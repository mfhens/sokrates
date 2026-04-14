package nl.obren.sokrates.reports.generators.statichtml;

import nl.obren.sokrates.sourcecode.SourceFile;
import nl.obren.sokrates.sourcecode.analysis.results.CodeAnalysisResults;
import nl.obren.sokrates.sourcecode.aspects.NamedSourceCodeAspect;
import nl.obren.sokrates.sourcecode.metrics.NumericMetric;

import java.util.*;

public class CommitTrendsExtractors {
    private CodeAnalysisResults analysisResults;

    public CommitTrendsExtractors(CodeAnalysisResults analysisResults) {
        this.analysisResults = analysisResults;
    }

    public Map<String, Map<String, Integer>> getCommitsPerYear(String logicalDecompositionKey) {
        List<SourceFile> allFiles = analysisResults.getFilesHistoryAnalysisResults().getAllFiles();
        if (analysisResults.getHistoryIndex() != null) {
            return analysisResults.getHistoryIndex().loadComponentCommitCountsByYear(allFiles, logicalDecompositionKey);
        }
        Map<String, Map<String, Integer>> componentsMap = new HashMap<>();
        allFiles.stream().filter(item -> item.getFileModificationHistory() != null).forEach(item -> {
            List<NamedSourceCodeAspect> logicalComponents = item.getLogicalComponents(logicalDecompositionKey);
            if (logicalComponents.size() > 0) {
                String componentName = logicalComponents.get(0).getName();

                Map<String, Integer> componentYears;
                if (componentsMap.containsKey(componentName)) {
                    componentYears = componentsMap.get(componentName);
                } else {
                    componentYears = new HashMap<>();
                    componentsMap.put(componentName, componentYears);
                }
                item.getFileModificationHistory().getCommits().forEach(commit -> {
                    String year = commit.getDate().substring(0, 4);
                    int prevValue = componentYears.getOrDefault(year, 0);
                    componentYears.put(year, prevValue + 1);
                });
            }
        });

        return componentsMap;
    }

    public List<NumericMetric> getTotalCommits(String logicalDecompositionKey) {
        List<SourceFile> allFiles = analysisResults.getFilesHistoryAnalysisResults().getAllFiles();
        Map<String, Integer> commitsMap = analysisResults.getHistoryIndex() != null
                ? analysisResults.getHistoryIndex().loadComponentCommitCounts(allFiles, logicalDecompositionKey)
                : new HashMap<>();
        if (analysisResults.getHistoryIndex() == null) {
        allFiles.stream().filter(item -> item.getFileModificationHistory() != null).forEach(item -> {
            List<NamedSourceCodeAspect> logicalComponents = item.getLogicalComponents(logicalDecompositionKey);
            if (logicalComponents.size() > 0) {
                String component = logicalComponents.get(0).getName();
                commitsMap.put(component, commitsMap.getOrDefault(component, 0) + item.getFileModificationHistory().getCommitsCount());
            }
        });
        }

        List<NumericMetric> metrics = new ArrayList<>();

        commitsMap.keySet().forEach(key -> {
            metrics.add(new NumericMetric(key, commitsMap.get(key)));
        });

        return metrics;
    }
}
