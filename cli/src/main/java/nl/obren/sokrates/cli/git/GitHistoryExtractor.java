package nl.obren.sokrates.cli.git;

import org.apache.commons.io.FileUtils;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.eclipse.jgit.api.Git;
import org.eclipse.jgit.api.errors.GitAPIException;
import org.eclipse.jgit.diff.DiffEntry;
import org.eclipse.jgit.diff.DiffFormatter;
import org.eclipse.jgit.lib.PersonIdent;
import org.eclipse.jgit.lib.Repository;
import org.eclipse.jgit.revwalk.RevCommit;
import org.eclipse.jgit.storage.file.FileRepositoryBuilder;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.Iterator;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

public class GitHistoryExtractor {
    private static final Log LOG = LogFactory.getLog(GitHistoryExtractor.class);
    private static final String HEADER_PREFIX = "__SOKRATES__";

    public enum Mode {
        COMPATIBILITY,
        LARGE_REPO
    }

    public void extractGitHistory(File root) {
        extractGitHistory(root, Mode.COMPATIBILITY, false, new File(root, "git-history.checkpoint"));
    }

    public void extractGitHistory(File root, Mode mode, boolean incremental, File checkpointFile) {
        if (mode == Mode.LARGE_REPO) {
            extractWithNativeGit(root, incremental, checkpointFile);
            return;
        }
        extractWithJGit(root);
    }

    private void extractWithJGit(File root) {
        FileRepositoryBuilder builder = new FileRepositoryBuilder();
        File gitHistoryFile = new File(root, "git-history.txt");
        try {
            LOG.info("Extracted git history...");
            Repository repo = builder.setGitDir(new File(root, ".git")).setMustExist(true).build();
            Git git = new Git(repo);
            Iterable<RevCommit> log = git.log().call();
            AtomicInteger count = new AtomicInteger();
            try (BufferedWriter writer = Files.newBufferedWriter(gitHistoryFile.toPath(), StandardCharsets.UTF_8)) {
                for (Iterator<RevCommit> iterator = log.iterator(); iterator.hasNext(); ) {
                    RevCommit rev = iterator.next();
                    if (rev.getParentCount() == 0) {
                        continue;
                    }
                    RevCommit prev = rev.getParent(0);
                    List<String> paths = new ArrayList<>();
                    try (DiffFormatter diffFormatter = new DiffFormatter(org.apache.commons.io.output.NullOutputStream.NULL_OUTPUT_STREAM)) {
                        diffFormatter.setRepository(repo);
                        for (DiffEntry entry : diffFormatter.scan(prev, rev)) {
                            String newPath = entry.getNewPath();
                            if (!newPath.equals("/dev/null")) {
                                paths.add(newPath);
                            }
                        }
                    }

                    for (String path : paths) {
                        SimpleDateFormat format = new SimpleDateFormat("yyyy-MM-dd");
                        PersonIdent authorIdent = rev.getAuthorIdent();
                        writer.write(toHistoryLine(format.format(authorIdent.getWhen()), authorIdent.getEmailAddress(), rev.getId().getName(), path, authorIdent.getName()));
                        writer.newLine();
                        count.incrementAndGet();
                    }
                }
            }
            LOG.info("Extracted " + count.get() + " file updates");
        } catch (IOException | GitAPIException e) {
            throw new IllegalStateException("Unable to extract git history with JGit", e);
        }
    }

    private void extractWithNativeGit(File root, boolean incremental, File checkpointFile) {
        File gitHistoryFile = new File(root, "git-history.txt");
        boolean canAppend = incremental && checkpointFile.exists() && gitHistoryFile.exists();
        String checkpointCommit = canAppend ? readCheckpoint(checkpointFile) : "";
        List<String> command = new ArrayList<>();
        command.add("git");
        command.add("log");
        command.add("--date=short");
        command.add("--name-only");
        command.add("--find-renames");
        command.add("--format=" + HEADER_PREFIX + "%ad%x09%ae%x09%H%x09%P%x09%an");
        if (canAppend && !checkpointCommit.isBlank()) {
            command.add(checkpointCommit + "..HEAD");
        } else {
            command.add("HEAD");
        }

        LOG.info("Extracting git history with native Git (" + (canAppend ? "incremental" : "full") + ")...");
        ProcessBuilder processBuilder = new ProcessBuilder(command);
        processBuilder.directory(root);
        processBuilder.redirectErrorStream(true);

        try {
            Process process = processBuilder.start();
            CommitBlock currentCommit = null;
            AtomicInteger updatesCount = new AtomicInteger();
            try (BufferedReader reader = new BufferedReader(new java.io.InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8));
                 BufferedWriter writer = Files.newBufferedWriter(
                         gitHistoryFile.toPath(),
                         StandardCharsets.UTF_8,
                         java.nio.file.StandardOpenOption.CREATE,
                         java.nio.file.StandardOpenOption.WRITE,
                         canAppend ? java.nio.file.StandardOpenOption.APPEND : java.nio.file.StandardOpenOption.TRUNCATE_EXISTING)) {
                String line;
                while ((line = reader.readLine()) != null) {
                    if (line.startsWith(HEADER_PREFIX)) {
                        updatesCount.addAndGet(writeCommit(writer, currentCommit));
                        currentCommit = parseCommit(line.substring(HEADER_PREFIX.length()));
                    } else if (currentCommit != null && !line.isBlank()) {
                        currentCommit.paths.add(line.trim());
                    }
                }
                updatesCount.addAndGet(writeCommit(writer, currentCommit));
            }
            int exitCode = process.waitFor();
            if (exitCode != 0) {
                throw new IllegalStateException("Native git history extraction failed with exit code " + exitCode);
            }
            writeCheckpoint(root, checkpointFile);
            LOG.info("Extracted " + updatesCount.get() + " file updates with native Git");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("Native git history extraction interrupted", e);
        } catch (IOException e) {
            throw new IllegalStateException("Unable to extract git history with native Git", e);
        }
    }

    private CommitBlock parseCommit(String header) {
        String[] elements = header.split("\\t", 5);
        CommitBlock block = new CommitBlock();
        block.date = elements.length > 0 ? elements[0] : "";
        block.email = elements.length > 1 ? elements[1] : "";
        block.commitId = elements.length > 2 ? elements[2] : "";
        block.parents = elements.length > 3 ? elements[3] : "";
        block.authorName = elements.length > 4 ? elements[4] : "";
        return block;
    }

    private int writeCommit(BufferedWriter writer, CommitBlock block) throws IOException {
        if (block == null || block.commitId.isBlank() || block.getParentCount() == 0 || block.getParentCount() > 1) {
            return 0;
        }
        int updates = 0;
        for (String path : block.paths) {
            writer.write(toHistoryLine(block.date, block.email, block.commitId, path, block.authorName));
            writer.newLine();
            updates += 1;
        }
        return updates;
    }

    private String toHistoryLine(String date, String email, String commitId, String path, String authorName) {
        String safePath = path.replace(" ", "&nbsp;");
        String safeName = authorName.replace(" ", "&nbsp;");
        return date + " " + email + " " + commitId + " " + safePath + " " + safeName;
    }

    private String readCheckpoint(File checkpointFile) {
        try {
            return Files.readString(checkpointFile.toPath(), StandardCharsets.UTF_8).trim();
        } catch (IOException e) {
            return "";
        }
    }

    private void writeCheckpoint(File root, File checkpointFile) throws IOException, InterruptedException {
        ProcessBuilder processBuilder = new ProcessBuilder("git", "rev-parse", "HEAD");
        processBuilder.directory(root);
        processBuilder.redirectErrorStream(true);
        Process process = processBuilder.start();
        String head;
        try (BufferedReader reader = new BufferedReader(new java.io.InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8))) {
            head = reader.readLine();
        }
        int exitCode = process.waitFor();
        if (exitCode == 0 && head != null) {
            FileUtils.writeStringToFile(checkpointFile, head.trim(), StandardCharsets.UTF_8);
        }
    }

    private static class CommitBlock {
        private String date = "";
        private String email = "";
        private String commitId = "";
        private String parents = "";
        private String authorName = "";
        private LinkedHashSet<String> paths = new LinkedHashSet<>();

        private int getParentCount() {
            return parents.isBlank() ? 0 : parents.trim().split("\\s+").length;
        }
    }
}
