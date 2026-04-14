package nl.obren.sokrates.sourcecode.githistory;

import nl.obren.sokrates.common.utils.ProcessingStopwatch;
import nl.obren.sokrates.sourcecode.SourceFile;
import nl.obren.sokrates.sourcecode.analysis.FileHistoryAnalysisConfig;
import nl.obren.sokrates.sourcecode.analysis.results.HistoryPerExtension;
import nl.obren.sokrates.sourcecode.contributors.ContributionTimeSlot;
import nl.obren.sokrates.sourcecode.contributors.Contributor;
import nl.obren.sokrates.sourcecode.contributors.ContributorsImport;
import nl.obren.sokrates.sourcecode.contributors.GitContributorsUtil;
import nl.obren.sokrates.sourcecode.dependencies.ComponentDependency;
import nl.obren.sokrates.sourcecode.filehistory.DateUtils;
import nl.obren.sokrates.sourcecode.filehistory.FileModificationHistory;
import nl.obren.sokrates.sourcecode.filehistory.FilePairChangedTogether;
import nl.obren.sokrates.sourcecode.operations.OperationStatement;
import nl.obren.sokrates.sourcecode.threshold.Thresholds;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;

import java.io.File;
import java.sql.*;
import java.util.*;
import java.util.stream.Collectors;

public class GitHistoryIndex {
    private static final Log LOG = LogFactory.getLog(GitHistoryIndex.class);
    private static final String VERSION = "2";

    private final File historyFile;
    private final FileHistoryAnalysisConfig config;
    private final File dbFile;

    private GitHistoryIndex(File historyFile, FileHistoryAnalysisConfig config) {
        this.historyFile = historyFile;
        this.config = config;
        this.dbFile = new File(historyFile.getParentFile(), historyFile.getName().replaceAll("[.]txt$", "") + ".sqlite");
    }

    public static GitHistoryIndex open(File historyFile, FileHistoryAnalysisConfig config) {
        GitHistoryIndex index = new GitHistoryIndex(historyFile, config);
        index.ensureReady();
        return index;
    }

    public boolean hasHistory() {
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement("SELECT 1 FROM file_stats LIMIT 1");
             ResultSet resultSet = statement.executeQuery()) {
            return resultSet.next();
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to inspect git history index", e);
        }
    }

    public List<FileModificationHistory> loadFileHistorySummaries(Collection<String> includedPaths) {
        Set<String> pathFilter = lowerCaseSet(includedPaths);
        List<FileModificationHistory> histories = new ArrayList<>();
        String sql = "SELECT path, oldest_date, latest_date, oldest_contributor, latest_contributor, " +
                "active_days_count, commits_count, contributors_count, " +
                "commits_count_30, commits_count_90, commits_count_180, commits_count_365, " +
                "contributors_count_30, contributors_count_90, contributors_count_180, contributors_count_365 " +
                "FROM file_stats";
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement(sql);
             ResultSet resultSet = statement.executeQuery()) {
            while (resultSet.next()) {
                String path = resultSet.getString("path");
                if (!pathFilter.isEmpty() && !pathFilter.contains(path.toLowerCase())) {
                    continue;
                }
                FileModificationHistory history = new FileModificationHistory(path);
                history.setOldestDate(resultSet.getString("oldest_date"));
                history.setLatestDate(resultSet.getString("latest_date"));
                history.setOldestContributor(resultSet.getString("oldest_contributor"));
                history.setLatestContributor(resultSet.getString("latest_contributor"));
                history.setActiveDaysCount(resultSet.getInt("active_days_count"));
                history.setCommitsCount(resultSet.getInt("commits_count"));
                history.setContributorsCount(resultSet.getInt("contributors_count"));
                history.setCommitsCount30Days(resultSet.getInt("commits_count_30"));
                history.setCommitsCount90Days(resultSet.getInt("commits_count_90"));
                history.setCommitsCount180Days(resultSet.getInt("commits_count_180"));
                history.setCommitsCount365Days(resultSet.getInt("commits_count_365"));
                history.setContributorsCount30Days(resultSet.getInt("contributors_count_30"));
                history.setContributorsCount90Days(resultSet.getInt("contributors_count_90"));
                history.setContributorsCount180Days(resultSet.getInt("contributors_count_180"));
                history.setContributorsCount365Days(resultSet.getInt("contributors_count_365"));
                histories.add(history);
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load file summaries from git history index", e);
        }
        return histories;
    }

    public List<HistoryPerExtension> loadHistoryPerExtensionPerYear() {
        Map<String, HistoryPerExtension> entries = new LinkedHashMap<>();
        try (Connection connection = openConnection()) {
            try (PreparedStatement statement = connection.prepareStatement(
                    "SELECT extension, year, commits_count FROM extension_year_stats ORDER BY year, extension");
                 ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    String extension = resultSet.getString("extension");
                    String year = resultSet.getString("year");
                    String key = extension + "::" + year;
                    HistoryPerExtension item = new HistoryPerExtension(extension, year, resultSet.getInt("commits_count"));
                    entries.put(key, item);
                }
            }
            try (PreparedStatement statement = connection.prepareStatement(
                    "SELECT extension, year, email FROM extension_year_contributors ORDER BY year, extension, email");
                 ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    String extension = resultSet.getString("extension");
                    String year = resultSet.getString("year");
                    HistoryPerExtension item = entries.get(extension + "::" + year);
                    if (item != null) {
                        item.getContributors().add(resultSet.getString("email"));
                    }
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load extension history from git history index", e);
        }
        return new ArrayList<>(entries.values());
    }

    public Map<String, Map<String, Integer>> loadCommitCountsByYearForPaths(Collection<String> includedPaths) {
        Set<String> pathFilter = lowerCaseSet(includedPaths);
        Map<String, Map<String, Integer>> result = new HashMap<>();
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement(
                     "SELECT path, year, commits_count FROM path_year_stats ORDER BY path, year");
             ResultSet resultSet = statement.executeQuery()) {
            while (resultSet.next()) {
                String path = resultSet.getString("path");
                if (!pathFilter.isEmpty() && !pathFilter.contains(path.toLowerCase())) {
                    continue;
                }
                result.computeIfAbsent(path, ignored -> new HashMap<>())
                        .put(resultSet.getString("year"), resultSet.getInt("commits_count"));
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load commit trends from git history index", e);
        }
        return result;
    }

    public Map<String, Map<String, Integer>> loadComponentCommitCountsByYear(List<SourceFile> sourceFiles, String logicalDecompositionKey) {
        Map<String, String> pathToComponent = toPathComponentMap(sourceFiles, logicalDecompositionKey);
        Map<String, Map<String, Integer>> result = new HashMap<>();
        if (pathToComponent.isEmpty()) {
            return result;
        }
        try (Connection connection = openConnection()) {
            populateIncludedComponents(connection, pathToComponent);
            try (PreparedStatement statement = connection.prepareStatement(
                    "SELECT m.component, substr(e.date, 1, 4) year, COUNT(DISTINCT e.commit_id) commits_count " +
                            "FROM events e JOIN temp_included_components m ON m.path = e.path " +
                            "GROUP BY m.component, year ORDER BY m.component, year");
                 ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    result.computeIfAbsent(resultSet.getString("component"), ignored -> new HashMap<>())
                            .put(resultSet.getString("year"), resultSet.getInt("commits_count"));
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load component commit trends", e);
        }
        return result;
    }

    public Map<String, Integer> loadComponentCommitCounts(List<SourceFile> sourceFiles, String logicalDecompositionKey) {
        Map<String, String> pathToComponent = toPathComponentMap(sourceFiles, logicalDecompositionKey);
        Map<String, Integer> result = new HashMap<>();
        if (pathToComponent.isEmpty()) {
            return result;
        }
        try (Connection connection = openConnection()) {
            populateIncludedComponents(connection, pathToComponent);
            try (PreparedStatement statement = connection.prepareStatement(
                    "SELECT m.component, COUNT(DISTINCT e.commit_id) commits_count " +
                            "FROM events e JOIN temp_included_components m ON m.path = e.path " +
                            "GROUP BY m.component ORDER BY m.component");
                 ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    result.put(resultSet.getString("component"), resultSet.getInt("commits_count"));
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load component commit totals", e);
        }
        return result;
    }

    public ContributorsImport loadContributorsImport() {
        List<AuthorCommit> authorCommits = loadAuthorCommits();
        ContributorsImport contributorsImport = new ContributorsImport();
        for (AuthorCommit commit : authorCommits) {
            String date = commit.getDate();
            if (contributorsImport.getFirstCommitDate().isBlank() || date.compareTo(contributorsImport.getFirstCommitDate()) <= 0) {
                contributorsImport.setFirstCommitDate(date);
            }
            if (contributorsImport.getLatestCommitDate().isBlank() || date.compareTo(contributorsImport.getLatestCommitDate()) >= 0) {
                contributorsImport.setLatestCommitDate(date);
            }
        }
        contributorsImport.setContributors(GitContributorsUtil.getContributors(authorCommits));
        contributorsImport.setContributorsPerYear(GitContributorsUtil.getContributorsPerTimeSlot(authorCommits, AuthorCommit::getYear));
        contributorsImport.setContributorsPerMonth(GitContributorsUtil.getContributorsPerTimeSlot(authorCommits, AuthorCommit::getMonth));
        contributorsImport.setContributorsPerWeek(GitContributorsUtil.getContributorsPerTimeSlot(authorCommits, AuthorCommit::getWeekOfYear));
        contributorsImport.setContributorsPerDay(GitContributorsUtil.getContributorsPerTimeSlot(authorCommits, AuthorCommit::getDate));
        return contributorsImport;
    }

    public List<CommitsPerExtension> loadCommitsPerExtensions() {
        Map<String, CommitsPerExtension> byExtension = new HashMap<>();
        try (Connection connection = openConnection()) {
            String totalsSql = "SELECT extension, COUNT(*) file_updates, " +
                    "COUNT(DISTINCT path) files_count, " +
                    "COUNT(DISTINCT CASE WHEN date >= ? THEN path END) files_count_30, " +
                    "COUNT(DISTINCT CASE WHEN date >= ? THEN path END) files_count_90 " +
                    "FROM events GROUP BY extension";
            try (PreparedStatement statement = connection.prepareStatement(totalsSql)) {
                statement.setString(1, daysAgo(30));
                statement.setString(2, daysAgo(90));
                try (ResultSet resultSet = statement.executeQuery()) {
                    while (resultSet.next()) {
                        String extension = resultSet.getString("extension");
                        CommitsPerExtension item = new CommitsPerExtension(extension);
                        item.setCommitsCount(resultSet.getInt("file_updates"));
                        item.setFilesCount(resultSet.getInt("files_count"));
                        item.setFilesCount30Days(resultSet.getInt("files_count_30"));
                        item.setFilesCount90Days(resultSet.getInt("files_count_90"));
                        byExtension.put(extension, item);
                    }
                }
            }

            String contributorSql = "SELECT extension, email, " +
                    "COUNT(*) file_updates, " +
                    "SUM(CASE WHEN date >= ? THEN 1 ELSE 0 END) file_updates_30, " +
                    "SUM(CASE WHEN date >= ? THEN 1 ELSE 0 END) file_updates_90 " +
                    "FROM events GROUP BY extension, email";
            try (PreparedStatement statement = connection.prepareStatement(contributorSql)) {
                statement.setString(1, daysAgo(30));
                statement.setString(2, daysAgo(90));
                try (ResultSet resultSet = statement.executeQuery()) {
                    while (resultSet.next()) {
                        String extension = resultSet.getString("extension");
                        CommitsPerExtension item = byExtension.computeIfAbsent(extension, CommitsPerExtension::new);
                        String email = resultSet.getString("email");
                        item.getCommitters().add(email);
                        if (resultSet.getInt("file_updates_30") > 0) {
                            item.getCommitters30Days().add(email);
                        }
                        if (resultSet.getInt("file_updates_90") > 0) {
                            item.getCommitters90Days().add(email);
                        }
                        ContributorPerExtensionStats stats = new ContributorPerExtensionStats(email);
                        stats.setFileUpdates(resultSet.getInt("file_updates"));
                        stats.setFileUpdates30Days(resultSet.getInt("file_updates_30"));
                        stats.setFileUpdates90Days(resultSet.getInt("file_updates_90"));
                        item.getContributorPerExtensionStats().add(stats);
                    }
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load per-extension commit statistics", e);
        }

        List<CommitsPerExtension> list = new ArrayList<>(byExtension.values());
        list.sort((a, b) -> b.getCommitters().size() - a.getCommitters().size());
        list.sort((a, b) -> b.getCommitters90Days().size() - a.getCommitters90Days().size());
        list.sort((a, b) -> b.getCommitters30Days().size() - a.getCommitters30Days().size());
        return list;
    }

    public List<FilePairChangedTogether> loadFilePairs(List<SourceFile> sourceFiles, int rangeInDays, Thresholds commitFilesThresholds, int limit) {
        ProcessingStopwatch.start("analysis/file history/pairs/" + rangeInDays + "d");
        Map<String, SourceFile> sourceFilesByPath = sourceFiles.stream()
                .collect(Collectors.toMap(file -> file.getRelativePath().toLowerCase(), file -> file, (left, right) -> left));
        Map<String, FileModificationHistory> fileStats = loadFileHistorySummaries(sourceFilesByPath.keySet())
                .stream()
                .collect(Collectors.toMap(item -> item.getPath().toLowerCase(), item -> item, (left, right) -> left));
        try (Connection connection = openConnection()) {
            populateIncludedPaths(connection, sourceFilesByPath);
            createWorkingPairTable(connection, "temp_file_pairs", "path1", "path2");
            String sql = "SELECT e.commit_id, e.date, e.path " +
                    "FROM events e " +
                    "JOIN commit_stats cs ON cs.commit_id = e.commit_id " +
                    "JOIN temp_included_paths p ON p.path = e.path " +
                    "WHERE cs.file_count <= ? " +
                    (rangeInDays > 0 ? "AND e.date >= ? " : "") +
                    "ORDER BY e.commit_id, e.path";
            try (PreparedStatement statement = connection.prepareStatement(sql)) {
                int index = 1;
                statement.setInt(index++, commitFilesThresholds.getMedium());
                if (rangeInDays > 0) {
                    statement.setString(index, daysAgo(rangeInDays));
                }
                try (ResultSet resultSet = statement.executeQuery()) {
                    consumeCommitPairs(resultSet, connection, "temp_file_pairs", null);
                }
            }
            List<FilePairChangedTogether> pairs = readFilePairs(connection, sourceFilesByPath, fileStats, limit);
            ProcessingStopwatch.end("analysis/file history/pairs/" + rangeInDays + "d");
            return pairs;
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to calculate temporal file pairs", e);
        }
    }

    public List<ComponentDependency> loadComponentDependencies(List<SourceFile> sourceFiles, String logicalDecompositionKey, int rangeInDays, Thresholds commitFilesThresholds, int limit) {
        ProcessingStopwatch.start("analysis/file history/component pairs/" + logicalDecompositionKey + "/" + rangeInDays + "d");
        Map<String, String> pathToComponent = new HashMap<>();
        for (SourceFile sourceFile : sourceFiles) {
            if (!sourceFile.getLogicalComponents(logicalDecompositionKey).isEmpty()) {
                pathToComponent.put(sourceFile.getRelativePath(), sourceFile.getLogicalComponents(logicalDecompositionKey).get(0).getName());
            }
        }
        try (Connection connection = openConnection()) {
            populateIncludedComponents(connection, pathToComponent);
            createWorkingPairTable(connection, "temp_component_pairs", "item1", "item2");
            String sql = "SELECT e.commit_id, e.date, m.component " +
                    "FROM events e " +
                    "JOIN commit_stats cs ON cs.commit_id = e.commit_id " +
                    "JOIN temp_included_components m ON m.path = e.path " +
                    "WHERE cs.file_count <= ? " +
                    (rangeInDays > 0 ? "AND e.date >= ? " : "") +
                    "ORDER BY e.commit_id, m.component";
            try (PreparedStatement statement = connection.prepareStatement(sql)) {
                int index = 1;
                statement.setInt(index++, commitFilesThresholds.getMedium());
                if (rangeInDays > 0) {
                    statement.setString(index, daysAgo(rangeInDays));
                }
                try (ResultSet resultSet = statement.executeQuery()) {
                    consumeCommitPairs(resultSet, connection, "temp_component_pairs", "component");
                }
            }
            List<ComponentDependency> dependencies = new ArrayList<>();
            try (PreparedStatement statement = connection.prepareStatement(
                    "SELECT item1, item2, shared_commits FROM temp_component_pairs " +
                            "ORDER BY shared_commits DESC LIMIT ?")) {
                statement.setInt(1, limit);
                try (ResultSet resultSet = statement.executeQuery()) {
                    while (resultSet.next()) {
                        ComponentDependency dependency = new ComponentDependency(resultSet.getString("item1"), resultSet.getString("item2"));
                        dependency.setCount(resultSet.getInt("shared_commits"));
                        dependencies.add(dependency);
                    }
                }
            }
            ProcessingStopwatch.end("analysis/file history/component pairs/" + logicalDecompositionKey + "/" + rangeInDays + "d");
            return dependencies;
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to calculate temporal component pairs", e);
        }
    }

    public List<ComponentDependency> loadPeopleDependencies(int daysAgo, int limit) {
        Map<String, List<String>> contributionMap = new HashMap<>();
        String sql = "SELECT path, email FROM events WHERE date >= ? GROUP BY path, email ORDER BY path, email";
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement(sql)) {
            statement.setString(1, daysAgo(daysAgo));
            try (ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    contributionMap.computeIfAbsent(resultSet.getString("path"), ignored -> new ArrayList<>())
                            .add(resultSet.getString("email"));
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to calculate contributor dependencies", e);
        }

        List<ComponentDependency> dependencies = new ArrayList<>();
        Map<String, ComponentDependency> dependenciesMap = new HashMap<>();
        for (Map.Entry<String, List<String>> entry : contributionMap.entrySet()) {
            List<String> emails = entry.getValue();
            for (int i = 0; i < emails.size(); i++) {
                for (int j = i + 1; j < emails.size(); j++) {
                    if (dependencies.size() > limit) {
                        return dependencies;
                    }
                    String email1 = emails.get(i);
                    String email2 = emails.get(j);
                    String key1 = email1 + "::" + email2;
                    String key2 = email2 + "::" + email1;
                    ComponentDependency dependency = dependenciesMap.containsKey(key1)
                            ? dependenciesMap.get(key1)
                            : dependenciesMap.get(key2);
                    if (dependency == null) {
                        dependency = new ComponentDependency(email1, email2);
                        dependenciesMap.put(key1, dependency);
                        dependencies.add(dependency);
                    }
                    if (!dependency.getData().contains(entry.getKey())) {
                        dependency.getData().add(entry.getKey());
                        dependency.setCount(dependency.getData().size());
                    }
                }
            }
        }
        dependencies.sort((a, b) -> b.getCount() - a.getCount());
        return dependencies;
    }

    public List<ComponentDependency> loadPeopleFileDependencies(int daysAgo, int limit) {
        List<ComponentDependency> dependencies = new ArrayList<>();
        String sql = "SELECT email, path, COUNT(*) updates FROM events WHERE date >= ? GROUP BY email, path ORDER BY updates DESC LIMIT ?";
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement(sql)) {
            statement.setString(1, daysAgo(daysAgo));
            statement.setInt(2, limit);
            try (ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    ComponentDependency dependency = new ComponentDependency(resultSet.getString("email"), "[" + resultSet.getString("path") + "]");
                    dependency.setCount(resultSet.getInt("updates"));
                    dependencies.add(dependency);
                }
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to calculate contributor/file dependencies", e);
        }
        return dependencies;
    }

    public File getDbFile() {
        return dbFile;
    }

    private List<AuthorCommit> loadAuthorCommits() {
        List<AuthorCommit> authorCommits = new ArrayList<>();
        String sql = "SELECT date, email, MAX(user_name) user_name, MAX(bot) bot, COUNT(*) file_updates " +
                "FROM events GROUP BY commit_id, date, email ORDER BY date, email";
        try (Connection connection = openConnection();
             PreparedStatement statement = connection.prepareStatement(sql);
             ResultSet resultSet = statement.executeQuery()) {
            while (resultSet.next()) {
                AuthorCommit commit = new AuthorCommit(
                        resultSet.getString("date"),
                        resultSet.getString("email"),
                        resultSet.getString("user_name"),
                        resultSet.getInt("bot") > 0
                );
                commit.setFileUpdatesCount(resultSet.getInt("file_updates"));
                authorCommits.add(commit);
            }
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to load author commits from git history index", e);
        }
        return authorCommits;
    }

    private void ensureReady() {
        try (Connection connection = openConnection()) {
            if (isRebuildNeeded(connection)) {
                connection.close();
                rebuild();
            }
        } catch (SQLException e) {
            rebuild();
        }
    }

    private boolean isRebuildNeeded(Connection connection) throws SQLException {
        if (!dbFile.exists()) {
            return true;
        }
        if (!tableExists(connection, "metadata") || !tableExists(connection, "events") || !tableExists(connection, "file_stats")) {
            return true;
        }
        Map<String, String> metadata = readMetadata(connection);
        return !VERSION.equals(metadata.get("version"))
                || !String.valueOf(historyFile.length()).equals(metadata.get("source_size"))
                || !String.valueOf(historyFile.lastModified()).equals(metadata.get("source_modified"))
                || !configSignature().equals(metadata.get("config_signature"));
    }

    private void rebuild() {
        LOG.info("Building git history index at " + dbFile.getPath());
        if (dbFile.exists() && !dbFile.delete()) {
            throw new IllegalStateException("Unable to replace git history index at " + dbFile.getPath());
        }
        try (Connection connection = openConnection()) {
            connection.setAutoCommit(false);
            createSchema(connection);
            insertEvents(connection);
            createDerivedTables(connection);
            storeMetadata(connection);
            connection.commit();
        } catch (SQLException e) {
            throw new IllegalStateException("Unable to build git history index", e);
        }
    }

    private void createSchema(Connection connection) throws SQLException {
        execute(connection, "CREATE TABLE IF NOT EXISTS events (" +
                "date TEXT NOT NULL, " +
                "email TEXT NOT NULL, " +
                "user_name TEXT, " +
                "commit_id TEXT NOT NULL, " +
                "path TEXT NOT NULL, " +
                "extension TEXT, " +
                "bot INTEGER NOT NULL DEFAULT 0)");
        execute(connection, "CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT)");
    }

    private void createDerivedTables(Connection connection) throws SQLException {
        String date30 = daysAgo(30);
        String date90 = daysAgo(90);
        String date180 = daysAgo(180);
        String date365 = daysAgo(365);

        execute(connection, "CREATE INDEX IF NOT EXISTS idx_events_path ON events(path)");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_events_commit ON events(commit_id)");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_events_date ON events(date)");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_events_extension ON events(extension)");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_events_email ON events(email)");

        execute(connection, "DROP TABLE IF EXISTS commit_stats");
        execute(connection, "CREATE TABLE commit_stats AS " +
                "SELECT commit_id, MIN(date) date, COUNT(DISTINCT path) file_count " +
                "FROM events GROUP BY commit_id");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_commit_stats_commit ON commit_stats(commit_id)");

        execute(connection, "DROP TABLE IF EXISTS file_stats");
        execute(connection, "CREATE TABLE file_stats AS " +
                "SELECT fs.path AS path, " +
                "MIN(fs.date) AS oldest_date, " +
                "MAX(fs.date) AS latest_date, " +
                "COUNT(DISTINCT fs.date) AS active_days_count, " +
                "COUNT(DISTINCT fs.commit_id) AS commits_count, " +
                "COUNT(DISTINCT fs.email) AS contributors_count, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date30 + "' THEN fs.commit_id END) AS commits_count_30, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date90 + "' THEN fs.commit_id END) AS commits_count_90, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date180 + "' THEN fs.commit_id END) AS commits_count_180, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date365 + "' THEN fs.commit_id END) AS commits_count_365, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date30 + "' THEN fs.email END) AS contributors_count_30, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date90 + "' THEN fs.email END) AS contributors_count_90, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date180 + "' THEN fs.email END) AS contributors_count_180, " +
                "COUNT(DISTINCT CASE WHEN fs.date >= '" + date365 + "' THEN fs.email END) AS contributors_count_365, " +
                "(SELECT e.email FROM events e WHERE e.path = fs.path ORDER BY e.date ASC, e.commit_id ASC LIMIT 1) AS oldest_contributor, " +
                "(SELECT e.email FROM events e WHERE e.path = fs.path ORDER BY e.date DESC, e.commit_id DESC LIMIT 1) AS latest_contributor " +
                "FROM events fs GROUP BY fs.path");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_file_stats_path ON file_stats(path)");

        execute(connection, "DROP TABLE IF EXISTS extension_year_stats");
        execute(connection, "CREATE TABLE extension_year_stats AS " +
                "SELECT extension, substr(date, 1, 4) year, COUNT(DISTINCT commit_id) commits_count " +
                "FROM events GROUP BY extension, year");
        execute(connection, "DROP TABLE IF EXISTS extension_year_contributors");
        execute(connection, "CREATE TABLE extension_year_contributors AS " +
                "SELECT extension, substr(date, 1, 4) year, email " +
                "FROM events GROUP BY extension, year, email");

        execute(connection, "DROP TABLE IF EXISTS path_year_stats");
        execute(connection, "CREATE TABLE path_year_stats AS " +
                "SELECT path, substr(date, 1, 4) year, COUNT(DISTINCT commit_id) commits_count " +
                "FROM events GROUP BY path, year");
        execute(connection, "CREATE INDEX IF NOT EXISTS idx_path_year_stats_path ON path_year_stats(path)");
    }

    private void insertEvents(Connection connection) throws SQLException {
        String sql = "INSERT INTO events(date, email, user_name, commit_id, path, extension, bot) VALUES (?, ?, ?, ?, ?, ?, ?)";
        try (PreparedStatement statement = connection.prepareStatement(sql)) {
            int batchSize = 0;
            GitHistoryUtils.streamHistoryFromFile(historyFile, config, fileUpdate -> {
                try {
                    statement.setString(1, fileUpdate.getDate());
                    statement.setString(2, fileUpdate.getAuthorEmail());
                    statement.setString(3, fileUpdate.getUserName());
                    statement.setString(4, fileUpdate.getCommitId());
                    statement.setString(5, fileUpdate.getPath());
                    statement.setString(6, fileUpdate.getExtension());
                    statement.setInt(7, fileUpdate.isBot() ? 1 : 0);
                    statement.addBatch();
                } catch (SQLException e) {
                    throw new IllegalStateException(e);
                }
            });
            statement.executeBatch();
        }
    }

    private void storeMetadata(Connection connection) throws SQLException {
        execute(connection, "DELETE FROM metadata");
        try (PreparedStatement statement = connection.prepareStatement("INSERT INTO metadata(key, value) VALUES (?, ?)")) {
            putMetadata(statement, "version", VERSION);
            putMetadata(statement, "source_size", String.valueOf(historyFile.length()));
            putMetadata(statement, "source_modified", String.valueOf(historyFile.lastModified()));
            putMetadata(statement, "config_signature", configSignature());
            statement.executeBatch();
        }
    }

    private void putMetadata(PreparedStatement statement, String key, String value) throws SQLException {
        statement.setString(1, key);
        statement.setString(2, value);
        statement.addBatch();
    }

    private Map<String, String> readMetadata(Connection connection) throws SQLException {
        Map<String, String> metadata = new HashMap<>();
        try (PreparedStatement statement = connection.prepareStatement("SELECT key, value FROM metadata");
             ResultSet resultSet = statement.executeQuery()) {
            while (resultSet.next()) {
                metadata.put(resultSet.getString("key"), resultSet.getString("value"));
            }
        }
        return metadata;
    }

    private boolean tableExists(Connection connection, String table) throws SQLException {
        try (PreparedStatement statement = connection.prepareStatement(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")) {
            statement.setString(1, table);
            try (ResultSet resultSet = statement.executeQuery()) {
                return resultSet.next();
            }
        }
    }

    private Connection openConnection() throws SQLException {
        return DriverManager.getConnection("jdbc:sqlite:" + dbFile.getAbsolutePath());
    }

    private void execute(Connection connection, String sql) throws SQLException {
        try (Statement statement = connection.createStatement()) {
            statement.execute(sql);
        }
    }

    private String configSignature() {
        StringBuilder builder = new StringBuilder();
        builder.append("date=").append(DateUtils.getAnalysisDate());
        builder.append("|ignore=").append(String.join(",", config.getIgnoreContributors()));
        builder.append("|bots=").append(String.join(",", config.getBots()));
        builder.append("|anon=").append(config.isAnonymizeContributors());
        builder.append("|ops=");
        for (OperationStatement operationStatement : config.getTransformContributorEmails()) {
            builder.append(operationStatement.getOp()).append(":");
            builder.append(String.join("~", operationStatement.getParams())).append(";");
        }
        return builder.toString();
    }

    private String daysAgo(int daysAgo) {
        Calendar calendar = DateUtils.getCalendar(DateUtils.getAnalysisDate());
        calendar.add(Calendar.DATE, -daysAgo);
        return String.format(Locale.ENGLISH, "%1$tY-%1$tm-%1$td", calendar.getTime());
    }

    private Set<String> lowerCaseSet(Collection<String> values) {
        if (values == null) {
            return Collections.emptySet();
        }
        return values.stream().map(String::toLowerCase).collect(Collectors.toSet());
    }

    private void populateIncludedPaths(Connection connection, Map<String, SourceFile> sourceFilesByPath) throws SQLException {
        execute(connection, "DROP TABLE IF EXISTS temp_included_paths");
        execute(connection, "CREATE TEMP TABLE temp_included_paths (path TEXT PRIMARY KEY)");
        try (PreparedStatement statement = connection.prepareStatement("INSERT INTO temp_included_paths(path) VALUES (?)")) {
            for (SourceFile sourceFile : sourceFilesByPath.values()) {
                statement.setString(1, sourceFile.getRelativePath());
                statement.addBatch();
            }
            statement.executeBatch();
        }
    }

    private void populateIncludedComponents(Connection connection, Map<String, String> pathToComponent) throws SQLException {
        execute(connection, "DROP TABLE IF EXISTS temp_included_components");
        execute(connection, "CREATE TEMP TABLE temp_included_components (path TEXT PRIMARY KEY, component TEXT NOT NULL)");
        try (PreparedStatement statement = connection.prepareStatement("INSERT INTO temp_included_components(path, component) VALUES (?, ?)")) {
            for (Map.Entry<String, String> entry : pathToComponent.entrySet()) {
                statement.setString(1, entry.getKey());
                statement.setString(2, entry.getValue());
                statement.addBatch();
            }
            statement.executeBatch();
        }
    }

    private Map<String, String> toPathComponentMap(List<SourceFile> sourceFiles, String logicalDecompositionKey) {
        Map<String, String> pathToComponent = new HashMap<>();
        for (SourceFile sourceFile : sourceFiles) {
            if (!sourceFile.getLogicalComponents(logicalDecompositionKey).isEmpty()) {
                pathToComponent.put(sourceFile.getRelativePath(), sourceFile.getLogicalComponents(logicalDecompositionKey).get(0).getName());
            }
        }
        return pathToComponent;
    }

    private void createWorkingPairTable(Connection connection, String tableName, String leftColumn, String rightColumn) throws SQLException {
        execute(connection, "DROP TABLE IF EXISTS " + tableName);
        execute(connection, "CREATE TEMP TABLE " + tableName + " (" +
                leftColumn + " TEXT NOT NULL, " +
                rightColumn + " TEXT NOT NULL, " +
                "shared_commits INTEGER NOT NULL DEFAULT 0, " +
                "latest_commit TEXT, " +
                "PRIMARY KEY(" + leftColumn + ", " + rightColumn + "))");
    }

    private void consumeCommitPairs(ResultSet resultSet, Connection connection, String tableName, String valueColumnName) throws SQLException {
        try (PreparedStatement insert = connection.prepareStatement(
                "INSERT INTO " + tableName + "(" + (valueColumnName == null ? "path1, path2" : "item1, item2") + ", shared_commits, latest_commit) " +
                        "VALUES (?, ?, 1, ?) " +
                        "ON CONFLICT(" + (valueColumnName == null ? "path1, path2" : "item1, item2") + ") DO UPDATE SET " +
                        "shared_commits = shared_commits + 1, " +
                        "latest_commit = CASE WHEN excluded.latest_commit > latest_commit THEN excluded.latest_commit ELSE latest_commit END")) {
            String currentCommit = null;
            String currentDate = null;
            LinkedHashSet<String> touchedItems = new LinkedHashSet<>();
            while (resultSet.next()) {
                String commitId = resultSet.getString("commit_id");
                if (currentCommit != null && !currentCommit.equals(commitId)) {
                    persistPairs(insert, touchedItems, currentDate);
                    touchedItems.clear();
                }
                currentCommit = commitId;
                currentDate = resultSet.getString("date");
                touchedItems.add(valueColumnName == null ? resultSet.getString("path") : resultSet.getString(valueColumnName));
            }
            if (!touchedItems.isEmpty()) {
                persistPairs(insert, touchedItems, currentDate);
            }
            insert.executeBatch();
        }
    }

    private void persistPairs(PreparedStatement insert, LinkedHashSet<String> touchedItems, String commitDate) throws SQLException {
        if (touchedItems.size() < 2) {
            return;
        }
        List<String> items = new ArrayList<>(touchedItems);
        for (int i = 0; i < items.size(); i++) {
            for (int j = i + 1; j < items.size(); j++) {
                String left = items.get(i);
                String right = items.get(j);
                if (left.compareToIgnoreCase(right) > 0) {
                    String swap = left;
                    left = right;
                    right = swap;
                }
                insert.setString(1, left);
                insert.setString(2, right);
                insert.setString(3, commitDate);
                insert.addBatch();
            }
        }
    }

    private List<FilePairChangedTogether> readFilePairs(Connection connection,
                                                        Map<String, SourceFile> sourceFilesByPath,
                                                        Map<String, FileModificationHistory> fileStats,
                                                        int limit) throws SQLException {
        List<FilePairChangedTogether> pairs = new ArrayList<>();
        String sql = "SELECT p.path1, p.path2, p.shared_commits, p.latest_commit, " +
                "fs1.commits_count commits_count_1, fs2.commits_count commits_count_2 " +
                "FROM temp_file_pairs p " +
                "JOIN file_stats fs1 ON fs1.path = p.path1 " +
                "JOIN file_stats fs2 ON fs2.path = p.path2 " +
                "ORDER BY " +
                "(CAST(p.shared_commits AS REAL) / NULLIF((fs1.commits_count + fs2.commits_count - p.shared_commits), 0)) DESC, " +
                "p.shared_commits DESC LIMIT ?";
        try (PreparedStatement statement = connection.prepareStatement(sql)) {
            statement.setInt(1, limit);
            try (ResultSet resultSet = statement.executeQuery()) {
                while (resultSet.next()) {
                    SourceFile sourceFile1 = sourceFilesByPath.get(resultSet.getString("path1").toLowerCase());
                    SourceFile sourceFile2 = sourceFilesByPath.get(resultSet.getString("path2").toLowerCase());
                    if (sourceFile1 == null || sourceFile2 == null) {
                        continue;
                    }
                    FilePairChangedTogether pair = new FilePairChangedTogether(sourceFile1, sourceFile2);
                    int sharedCommits = resultSet.getInt("shared_commits");
                    int commitsCountFile1 = resultSet.getInt("commits_count_1");
                    int commitsCountFile2 = resultSet.getInt("commits_count_2");
                    pair.setSharedCommitsCount(sharedCommits);
                    pair.setCommitsCountFile1(commitsCountFile1);
                    pair.setCommitsCountFile2(commitsCountFile2);
                    pair.setLatestCommit(resultSet.getString("latest_commit"));
                    pair.setConfidenceFile1(confidence(sharedCommits, commitsCountFile1));
                    pair.setConfidenceFile2(confidence(sharedCommits, commitsCountFile2));
                    pair.setJaccardScore(jaccard(sharedCommits, commitsCountFile1, commitsCountFile2));
                    pair.setNormalizedScore((pair.getConfidenceFile1() + pair.getConfidenceFile2() + pair.getJaccardScore()) / 3.0);
                    if (fileStats.containsKey(pair.getSourceFile1().getRelativePath().toLowerCase())) {
                        pair.setCommitsCountFile1(fileStats.get(pair.getSourceFile1().getRelativePath().toLowerCase()).getCommitsCount());
                    }
                    if (fileStats.containsKey(pair.getSourceFile2().getRelativePath().toLowerCase())) {
                        pair.setCommitsCountFile2(fileStats.get(pair.getSourceFile2().getRelativePath().toLowerCase()).getCommitsCount());
                    }
                    pairs.add(pair);
                }
            }
        }
        return pairs;
    }

    private double confidence(int sharedCommits, int fileCommits) {
        return fileCommits <= 0 ? 0.0 : (double) sharedCommits / fileCommits;
    }

    private double jaccard(int sharedCommits, int commitsCountFile1, int commitsCountFile2) {
        int union = commitsCountFile1 + commitsCountFile2 - sharedCommits;
        return union <= 0 ? 0.0 : (double) sharedCommits / union;
    }
}
