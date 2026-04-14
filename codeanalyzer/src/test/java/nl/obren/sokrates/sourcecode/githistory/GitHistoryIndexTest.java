package nl.obren.sokrates.sourcecode.githistory;

import nl.obren.sokrates.sourcecode.SourceFile;
import nl.obren.sokrates.sourcecode.analysis.FileHistoryAnalysisConfig;
import nl.obren.sokrates.sourcecode.analysis.results.HistoryPerExtension;
import nl.obren.sokrates.sourcecode.filehistory.FileModificationHistory;
import nl.obren.sokrates.sourcecode.filehistory.FilePairChangedTogether;
import nl.obren.sokrates.sourcecode.threshold.Thresholds;
import org.apache.commons.io.FileUtils;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.File;
import java.nio.charset.StandardCharsets;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class GitHistoryIndexTest {
    @TempDir
    File tempDir;

    @Test
    void loadsAggregatedFileSummariesFromIndex() throws Exception {
        GitHistoryIndex index = GitHistoryIndex.open(writeHistoryFile(), new FileHistoryAnalysisConfig());

        List<FileModificationHistory> histories = index.loadFileHistorySummaries(List.of("src/A.java", "src/B.java"));
        assertEquals(2, histories.size());

        FileModificationHistory aHistory = histories.stream()
                .filter(history -> history.getPath().equals("src/A.java"))
                .findFirst()
                .orElseThrow();
        assertEquals(4, aHistory.getActiveDaysCount());
        assertEquals(4, aHistory.getCommitsCount());
        assertEquals(4, aHistory.getContributorsCount());

        List<HistoryPerExtension> extensionHistory = index.loadHistoryPerExtensionPerYear();
        HistoryPerExtension java2020 = extensionHistory.stream()
                .filter(item -> item.getExtension().equals("java"))
                .filter(item -> item.getYear().equals("2020"))
                .findFirst()
                .orElseThrow();
        assertEquals(4, java2020.getCommitsCount());
        assertEquals(4, java2020.getContributors().size());
    }

    @Test
    void skipsOversizedCommitsWhenCalculatingPairs() throws Exception {
        GitHistoryIndex index = GitHistoryIndex.open(writeHistoryFile(), new FileHistoryAnalysisConfig());

        SourceFile fileA = sourceFile("src/A.java");
        SourceFile fileB = sourceFile("src/B.java");
        SourceFile fileC = sourceFile("src/C.java");

        List<FilePairChangedTogether> pairs = index.loadFilePairs(
                List.of(fileA, fileB, fileC),
                0,
                new Thresholds(1, 1, 1, 2),
                20);

        FilePairChangedTogether pairAB = pairs.stream()
                .filter(pair -> pair.getSourceFile1().getRelativePath().equals("src/A.java") || pair.getSourceFile2().getRelativePath().equals("src/A.java"))
                .filter(pair -> pair.getSourceFile1().getRelativePath().equals("src/B.java") || pair.getSourceFile2().getRelativePath().equals("src/B.java"))
                .findFirst()
                .orElseThrow();

        assertEquals(2, pairAB.getSharedCommitsCount());
        assertTrue(pairAB.getNormalizedScore() > 0.0);
    }

    private File writeHistoryFile() throws Exception {
        File historyFile = new File(tempDir, "git-history.txt");
        String content = String.join("\n",
                "2020-04-01 alice@example.com c1 src/A.java Alice",
                "2020-04-01 alice@example.com c1 src/B.java Alice",
                "2020-03-01 bob@example.com c2 src/A.java Bob",
                "2020-02-20 carol@example.com c3 docs/readme.md Carol",
                "2020-01-15 bot@example.com c4 src/A.java Build&nbsp;Bot",
                "2020-01-15 bot@example.com c4 src/B.java Build&nbsp;Bot",
                "2020-01-10 dave@example.com c5 src/A.java Dave",
                "2020-01-10 dave@example.com c5 src/B.java Dave",
                "2020-01-10 dave@example.com c5 src/C.java Dave",
                "");
        FileUtils.writeStringToFile(historyFile, content, StandardCharsets.UTF_8);
        return historyFile;
    }

    private SourceFile sourceFile(String relativePath) {
        SourceFile sourceFile = new SourceFile();
        sourceFile.setRelativePath(relativePath);
        return sourceFile;
    }
}
