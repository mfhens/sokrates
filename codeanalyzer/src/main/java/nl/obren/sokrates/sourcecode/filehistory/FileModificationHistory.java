/*
 * Copyright (c) 2020 Željko Obrenović. All rights reserved.
 */

package nl.obren.sokrates.sourcecode.filehistory;

import java.util.*;
import java.util.stream.Stream;

public class FileModificationHistory {
    private List<String> dates = new ArrayList<>();
    private List<CommitInfo> commits = new ArrayList<>();
    private String path = "";
    private boolean sorted = false;
    private String oldestDate = "";
    private String latestDate = "";
    private String oldestContributor = "";
    private String latestContributor = "";
    private int activeDaysCount = 0;
    private int commitsCount = 0;
    private int contributorsCount = 0;
    private int commitsCount30Days = 0;
    private int commitsCount90Days = 0;
    private int commitsCount180Days = 0;
    private int commitsCount365Days = 0;
    private int contributorsCount30Days = 0;
    private int contributorsCount90Days = 0;
    private int contributorsCount180Days = 0;
    private int contributorsCount365Days = 0;

    public FileModificationHistory() {
    }

    public FileModificationHistory(String path) {
        this.path = path;
    }

    public String getPath() {
        return path;
    }

    public void setPath(String path) {
        this.path = path;
    }

    public List<String> getDates() {
        return dates;
    }

    public void setDates(List<String> dates) {
        this.dates = dates;
    }

    public String getOldestDate() {
        if (!oldestDate.isBlank()) {
            return oldestDate;
        }
        sortOldestFirst();
        return dates.get(0);
    }

    public String getLatestDate() {
        if (!latestDate.isBlank()) {
            return latestDate;
        }
        sortOldestFirst();
        return dates.get(dates.size() - 1);
    }

    public String getOldestContributor() {
        if (!oldestContributor.isBlank()) {
            return oldestContributor;
        }
        sortOldestFirst();
        return commits.get(0).getEmail();
    }

    public String getLatestContributor() {
        if (!latestContributor.isBlank()) {
            return latestContributor;
        }
        sortOldestFirst();
        return commits.get(commits.size() - 1).getEmail();
    }

    public void sortOldestFirst() {
        if (!sorted) {
            sorted = true;
            Collections.sort(dates);
            Collections.sort(commits, (a, b) -> a.getDate().compareTo(b.getDate()));
        }
    }

    public int daysSinceFirstUpdate() {
        return FileHistoryUtils.daysFromToday(getOldestDate());
    }

    public int daysSinceLatestUpdate() {
        return FileHistoryUtils.daysFromToday(getLatestDate());
    }

    public List<CommitInfo> getCommits() {
        return commits;
    }

    public void setCommits(List<CommitInfo> commits) {
        this.commits = commits;
    }

    public int countContributors() {
        if (contributorsCount > 0 || commits.isEmpty()) {
            return contributorsCount;
        }
        Set<String> contributorIds = new HashSet<>();
        commits.forEach(commit -> contributorIds.add(commit.getEmail()));
        contributorsCount = contributorIds.size();
        return contributorsCount;
    }

    public void registerCommit(CommitInfo commitInfo) {
        commits.add(commitInfo);
        commitsCount += 1;

        String date = commitInfo.getDate();
        if (!dates.contains(date)) {
            dates.add(date);
            activeDaysCount += 1;
        }

        String email = commitInfo.getEmail();
        if (oldestDate.isBlank() || date.compareTo(oldestDate) < 0) {
            oldestDate = date;
            oldestContributor = email;
        }
        if (latestDate.isBlank() || date.compareTo(latestDate) > 0) {
            latestDate = date;
            latestContributor = email;
        }
        sorted = false;
    }

    public int getActiveDaysCount() {
        if (activeDaysCount > 0 || dates.isEmpty()) {
            return activeDaysCount;
        }
        activeDaysCount = dates.size();
        return activeDaysCount;
    }

    public void setActiveDaysCount(int activeDaysCount) {
        this.activeDaysCount = activeDaysCount;
    }

    public int getCommitsCount() {
        if (commitsCount > 0 || commits.isEmpty()) {
            return commitsCount;
        }
        commitsCount = commits.size();
        return commitsCount;
    }

    public void setCommitsCount(int commitsCount) {
        this.commitsCount = commitsCount;
    }

    public int getContributorsCount() {
        return countContributors();
    }

    public void setContributorsCount(int contributorsCount) {
        this.contributorsCount = contributorsCount;
    }

    public String getOldestDateValue() {
        return oldestDate;
    }

    public void setOldestDate(String oldestDate) {
        this.oldestDate = oldestDate;
    }

    public String getLatestDateValue() {
        return latestDate;
    }

    public void setLatestDate(String latestDate) {
        this.latestDate = latestDate;
    }

    public void setOldestContributor(String oldestContributor) {
        this.oldestContributor = oldestContributor;
    }

    public void setLatestContributor(String latestContributor) {
        this.latestContributor = latestContributor;
    }

    public int getCommitsCount30Days() {
        return commitsCount30Days;
    }

    public void setCommitsCount30Days(int commitsCount30Days) {
        this.commitsCount30Days = commitsCount30Days;
    }

    public int getCommitsCount90Days() {
        return commitsCount90Days;
    }

    public void setCommitsCount90Days(int commitsCount90Days) {
        this.commitsCount90Days = commitsCount90Days;
    }

    public int getCommitsCount180Days() {
        return commitsCount180Days;
    }

    public void setCommitsCount180Days(int commitsCount180Days) {
        this.commitsCount180Days = commitsCount180Days;
    }

    public int getCommitsCount365Days() {
        return commitsCount365Days;
    }

    public void setCommitsCount365Days(int commitsCount365Days) {
        this.commitsCount365Days = commitsCount365Days;
    }

    public int getContributorsCount30Days() {
        return contributorsCount30Days;
    }

    public void setContributorsCount30Days(int contributorsCount30Days) {
        this.contributorsCount30Days = contributorsCount30Days;
    }

    public int getContributorsCount90Days() {
        return contributorsCount90Days;
    }

    public void setContributorsCount90Days(int contributorsCount90Days) {
        this.contributorsCount90Days = contributorsCount90Days;
    }

    public int getContributorsCount180Days() {
        return contributorsCount180Days;
    }

    public void setContributorsCount180Days(int contributorsCount180Days) {
        this.contributorsCount180Days = contributorsCount180Days;
    }

    public int getContributorsCount365Days() {
        return contributorsCount365Days;
    }

    public void setContributorsCount365Days(int contributorsCount365Days) {
        this.contributorsCount365Days = contributorsCount365Days;
    }
}
