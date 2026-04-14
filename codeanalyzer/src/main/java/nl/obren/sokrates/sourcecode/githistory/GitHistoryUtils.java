package nl.obren.sokrates.sourcecode.githistory;

/*
 * Assumes that you have generated the text file being read using the following git command:
 * git ls-files -z | xargs -0 -n1 -I{} -- git log --date=short --format="%ad %ae %H {}" {} > git-history.txt
 * git log --merges --first-parent --date=short --format="%ad %ae" > git-merges.txt
 */

import nl.obren.sokrates.common.utils.RegexUtils;
import nl.obren.sokrates.sourcecode.analysis.FileHistoryAnalysisConfig;
import nl.obren.sokrates.sourcecode.filehistory.DateUtils;
import nl.obren.sokrates.sourcecode.operations.ComplexOperation;
import org.apache.commons.lang3.StringUtils;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;

import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.*;
import java.util.function.Consumer;

public class GitHistoryUtils {
    public static final String GIT_HISTORY_FILE_NAME = "git-history.txt";
    public static final String EARLIEST_DATE = "1980-01-01";
    private static final Log LOG = LogFactory.getLog(GitHistoryUtils.class);

    public static String printContributorsCommand() {
        return "git ls-files -z | xargs -0 -n1 -I{} -- git log --date=short --format=\"%ad %ae %H {}\" {} > " + GIT_HISTORY_FILE_NAME;
    }

    public static List<AuthorCommit> getAuthorCommits(File file, FileHistoryAnalysisConfig config) {
        List<AuthorCommit> commits = new ArrayList<>();
        Set<String> commitIds = new HashSet<>();
        Map<String, AuthorCommit> commitsMap = new HashMap<>();

        int index[] = {0};
        streamHistoryFromFile(file, config, fileUpdate -> {
            index[0] += 1;
            if (index[0] % 1000 == 1) {
                LOG.info("Importing " + fileUpdate.getAuthorEmail() + " " + fileUpdate.getDate() + " (" + index[0] + ")");
            }
            String commitId = fileUpdate.getCommitId();
            if (!commitIds.contains(commitId)) {
                commitIds.add(commitId);
                AuthorCommit authorCommit = new AuthorCommit(fileUpdate.getDate(), fileUpdate.getAuthorEmail(), fileUpdate.getUserName(), fileUpdate.isBot());
                commits.add(authorCommit);
                commitsMap.put(commitId, authorCommit);
            } else {
                commitsMap.get(commitId).incrementFileUpdatesCount();
            }
        });

        return commits;
    }

    public static boolean shouldIgnore(String email, List<String> ignoreContributors) {
        for (String ignorePattern : ignoreContributors) {
            if (RegexUtils.matchesEntirely(ignorePattern.toLowerCase(), email.toLowerCase())) {
                return true;
            }
        }
        return false;
    }

    public static List<FileUpdate> getHistoryFromFile(File file, FileHistoryAnalysisConfig config) {
        List<FileUpdate> updates = new ArrayList<>();
        streamHistoryFromFile(file, config, updates::add);
        return updates;
    }

    public static void streamHistoryFromFile(File file, FileHistoryAnalysisConfig config, Consumer<FileUpdate> consumer) {
        LOG.info("Reading history from file");
        Map<String, String> anonymizeEmails = new HashMap<>();
        int displayCounter[] = {0};
        try (BufferedReader reader = Files.newBufferedReader(file.toPath(), StandardCharsets.UTF_8)) {
            String line;
            while ((line = reader.readLine()) != null) {
                displayCounter[0] += 1;
                if (displayCounter[0] % 1000 == 1) {
                    LOG.info("Reading commit line " + displayCounter[0] + ": " + StringUtils.abbreviate(line, 64));
                }
                FileUpdate fileUpdate = GitHistoryUtils.parseLine(line, config, anonymizeEmails);
                if (fileUpdate != null) {
                    fileUpdate.setBot(isBot(fileUpdate.getAuthorEmail(), config.getBots()));
                    consumer.accept(fileUpdate);
                }
            }
        } catch (IOException e) {
            LOG.info(e.getMessage());
        }
    }

    public static FileUpdate parseLine(String line, FileHistoryAnalysisConfig config) {
        return parseLine(line, config, new HashMap<>());
    }

    public static FileUpdate parseLine(String line, FileHistoryAnalysisConfig config, Map<String, String> anonymizeEmails) {
        List<String> ignoreContributors = config.getIgnoreContributors();
        boolean anonymize = config.isAnonymizeContributors();

        int index1 = line.indexOf(" ");
        if (index1 >= 10) {
            int index2 = line.indexOf(" ", index1 + 1);
            if (index2 > 0) {
                int index3 = line.indexOf(" ", index2 + 1);
                if (index3 > 0) {
                    String date = line.substring(0, 10).trim();
                    if (ignoreCommitByDate(line, date)) {
                        return null;
                    }
                    String authorEmail = line.substring(index1 + 1, index2).trim().toLowerCase();
                    if (shouldIgnore(authorEmail, ignoreContributors)) {
                        return null;
                    }
                    boolean bot = isBot(authorEmail, config.getBots());
                    if (anonymize) {
                        String anonymizedAuthor = anonymizeEmails.get(authorEmail);
                        if (anonymizedAuthor == null) {
                            anonymizedAuthor = "Contributor " + (anonymizeEmails.keySet().size() + 1);
                            anonymizeEmails.put(authorEmail, anonymizedAuthor);
                        }
                        authorEmail = anonymizedAuthor;
                    } else if (config.getTransformContributorEmails().size() > 0) {
                        ComplexOperation operation = new ComplexOperation(config.getTransformContributorEmails());
                        String original = authorEmail;
                        authorEmail = operation.exec(authorEmail);
                        if (shouldIgnore(authorEmail, ignoreContributors)) {
                            return null;
                        }
                    }

                    String commitId = line.substring(index2 + 1, index3).trim();
                    String path = line.substring(index3 + 1).replaceAll(" .*", "").replaceAll("[&]nbsp[;]", " ").trim();

                    int index4 = line.indexOf(" ", index3 + 1);

                    String userName = "";
                    if (index4 > index3) {
                        userName = line.substring(index4 + 1).replaceAll(" .*", "").replaceAll("[&]nbsp[;]", " ").trim();
                    }


                    bot = bot || isBot(authorEmail, config.getBots());

                    FileUpdate fileUpdate = new FileUpdate(date, authorEmail, userName, commitId, path, bot);
                    return fileUpdate;
                }
            }
        }

        return null;
    }

    private static boolean ignoreCommitByDate(String line, String date) {
        if (date.compareTo(DateUtils.getAnalysisDate()) > 0) {
            LOG.info("Ignoring future date: " + line);
            return true;
        }
        if (date.compareTo(EARLIEST_DATE) < 0) {
            LOG.info("Ignoring dates before the initial git release: " + line);
            return true;
        }
        return false;
    }

    public static boolean isBot(String email, List<String> bots) {
        return RegexUtils.matchesAnyPattern(email, bots);
    }
}
