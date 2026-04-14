/*
 * Copyright (c) 2020 Željko Obrenović. All rights reserved.
 */

package nl.obren.sokrates.sourcecode.filehistory;

import nl.obren.sokrates.sourcecode.SourceFile;

import java.util.ArrayList;
import java.util.List;

public class FilePairChangedTogether {
    private SourceFile sourceFile1;
    private SourceFile sourceFile2;
    private int commitsCountFile1;
    private int commitsCountFile2;
    private String latestCommit = "";
    private int sharedCommitsCount = 0;
    private double confidenceFile1 = 0.0;
    private double confidenceFile2 = 0.0;
    private double jaccardScore = 0.0;
    private double normalizedScore = 0.0;

    private List<String> commits = new ArrayList<>();

    public FilePairChangedTogether() {
    }

    public FilePairChangedTogether(SourceFile sourceFile1, SourceFile sourceFile2) {
        this.sourceFile1 = sourceFile1;
        this.sourceFile2 = sourceFile2;
    }

    public SourceFile getSourceFile1() {
        return sourceFile1;
    }

    public void setSourceFile1(SourceFile sourceFile1) {
        this.sourceFile1 = sourceFile1;
    }

    public SourceFile getSourceFile2() {
        return sourceFile2;
    }

    public void setSourceFile2(SourceFile sourceFile2) {
        this.sourceFile2 = sourceFile2;
    }

    public List<String> getCommits() {
        return commits;
    }

    public void setCommits(List<String> commits) {
        this.commits = commits;
        this.sharedCommitsCount = commits.size();
    }

    public int getCommitsCountFile1() {
        return commitsCountFile1;
    }

    public void setCommitsCountFile1(int commitsCountFile1) {
        this.commitsCountFile1 = commitsCountFile1;
    }

    public int getCommitsCountFile2() {
        return commitsCountFile2;
    }

    public void setCommitsCountFile2(int commitsCountFile2) {
        this.commitsCountFile2 = commitsCountFile2;
    }

    public String getLatestCommit() {
        return latestCommit;
    }

    public void setLatestCommit(String latestCommit) {
        this.latestCommit = latestCommit;
    }

    public int getSharedCommitsCount() {
        return sharedCommitsCount > 0 ? sharedCommitsCount : commits.size();
    }

    public void setSharedCommitsCount(int sharedCommitsCount) {
        this.sharedCommitsCount = sharedCommitsCount;
    }

    public double getConfidenceFile1() {
        return confidenceFile1;
    }

    public void setConfidenceFile1(double confidenceFile1) {
        this.confidenceFile1 = confidenceFile1;
    }

    public double getConfidenceFile2() {
        return confidenceFile2;
    }

    public void setConfidenceFile2(double confidenceFile2) {
        this.confidenceFile2 = confidenceFile2;
    }

    public double getJaccardScore() {
        return jaccardScore;
    }

    public void setJaccardScore(double jaccardScore) {
        this.jaccardScore = jaccardScore;
    }

    public double getNormalizedScore() {
        return normalizedScore;
    }

    public void setNormalizedScore(double normalizedScore) {
        this.normalizedScore = normalizedScore;
    }
}
