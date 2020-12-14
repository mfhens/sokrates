/*
 * Copyright (c) 2019 Željko Obrenović. All rights reserved.
 */

package nl.obren.sokrates.reports.landscape.statichtml;

import nl.obren.sokrates.common.utils.FormattingUtils;
import nl.obren.sokrates.reports.core.ReportFileExporter;
import nl.obren.sokrates.reports.core.RichTextReport;
import nl.obren.sokrates.reports.landscape.data.LandscapeDataExport;
import nl.obren.sokrates.reports.utils.GraphvizDependencyRenderer;
import nl.obren.sokrates.sourcecode.Metadata;
import nl.obren.sokrates.sourcecode.analysis.results.AspectAnalysisResults;
import nl.obren.sokrates.sourcecode.analysis.results.CodeAnalysisResults;
import nl.obren.sokrates.sourcecode.contributors.ContributionYear;
import nl.obren.sokrates.sourcecode.contributors.Contributor;
import nl.obren.sokrates.sourcecode.dependencies.ComponentDependency;
import nl.obren.sokrates.sourcecode.filehistory.DateUtils;
import nl.obren.sokrates.sourcecode.githistory.CommitsPerExtension;
import nl.obren.sokrates.sourcecode.landscape.*;
import nl.obren.sokrates.sourcecode.landscape.analysis.ContributorProjectInfo;
import nl.obren.sokrates.sourcecode.landscape.analysis.ContributorProjects;
import nl.obren.sokrates.sourcecode.landscape.analysis.LandscapeAnalysisResults;
import nl.obren.sokrates.sourcecode.landscape.analysis.ProjectAnalysisResults;
import nl.obren.sokrates.sourcecode.metrics.NumericMetric;
import org.apache.commons.lang3.StringUtils;
import org.apache.commons.logging.Log;
import org.apache.commons.logging.LogFactory;
import org.apache.commons.text.StringEscapeUtils;

import java.io.File;
import java.text.SimpleDateFormat;
import java.util.*;
import java.util.stream.Collectors;

public class LandscapeReportGenerator {
    public static final int RECENT_THRESHOLD_DAYS = 40;
    private static final Log LOG = LogFactory.getLog(LandscapeReportGenerator.class);
    private RichTextReport landscapeReport = new RichTextReport("Landscape Report", "index.html");
    private LandscapeAnalysisResults landscapeAnalysisResults;
    private int dependencyVisualCounter = 1;

    public LandscapeReportGenerator(LandscapeAnalysisResults landscapeAnalysisResults, File folder) {
        LandscapeDataExport dataExport = new LandscapeDataExport(landscapeAnalysisResults, folder);
        dataExport.exportProjects();
        dataExport.exportContributors();
        dataExport.exportAnalysisResults();

        Metadata metadata = landscapeAnalysisResults.getConfiguration().getMetadata();
        String landscapeName = metadata.getName();
        if (StringUtils.isNotBlank(landscapeName)) {
            landscapeReport.setDisplayName(landscapeName);
        }
        landscapeReport.setParentUrl(landscapeAnalysisResults.getConfiguration().getParentUrl());
        landscapeReport.setLogoLink(metadata.getLogoLink());
        String description = metadata.getDescription();
        if (StringUtils.isNotBlank(description)) {
            landscapeReport.addParagraph(description);
        }
        if (metadata.getLinks().size() > 0) {
            landscapeReport.startDiv("");
            boolean first[] = {true};
            metadata.getLinks().forEach(link -> {
                if (!first[0]) {
                    landscapeReport.addHtmlContent(" | ");
                }
                landscapeReport.addNewTabLink(link.getLabel(), link.getHref());
                first[0] = false;
            });
            landscapeReport.endDiv();
        }
        this.landscapeAnalysisResults = landscapeAnalysisResults;

        landscapeReport.addLineBreak();
        landscapeReport.startTabGroup();
        landscapeReport.addTab("overview", "Overview", true);
        landscapeReport.addTab("source code", "Projects", false);
        landscapeReport.addTab("commits", "Contributors", false);
        landscapeReport.endTabGroup();

        landscapeReport.startTabContentSection("overview", true);
        addBigSummary(landscapeAnalysisResults);
        addSubLandscapeSection(landscapeAnalysisResults.getConfiguration().getSubLandscapes());
        landscapeReport.endTabContentSection();
        landscapeReport.startTabContentSection("source code", false);
        addBigProjectsSummary(landscapeAnalysisResults);
        addExtensions();
        addProjectsSection(getProjects());
        landscapeReport.endTabContentSection();

        landscapeReport.startTabContentSection("commits", false);
        addBigContributorsSummary(landscapeAnalysisResults);
        addContributors();
        addContributorsPerExtension();
        addPeopleDependencies();
        landscapeReport.endTabContentSection();
        landscapeReport.addParagraph("<span style='color: grey; font-size: 90%'>updated: " + new SimpleDateFormat("yyyy-MM-dd").format(new Date()) + "</span>");
    }

    private void addPeopleDependencies() {
        landscapeReport.startSubSection("People Dependencies", "");
        this.renderPeopleDependencies(30);
        this.renderPeopleDependencies(90);
        this.renderPeopleDependencies(180);
        landscapeReport.endSection();
    }

    private List<ProjectAnalysisResults> getProjects() {
        return landscapeAnalysisResults.getFilteredProjectAnalysisResults();
    }

    private void addSubLandscapeSection(List<SubLandscapeLink> subLandscapes) {
        List<SubLandscapeLink> links = new ArrayList<>(subLandscapes);
        if (links.size() > 0) {
            Collections.sort(links, (a, b) -> getLabel(a).compareTo(getLabel(b)));
            landscapeReport.startSubSection("Sub-Landscapes (" + links.size() + ")", "");

            landscapeReport.startUnorderedList();

            links.forEach(subLandscape -> {
                landscapeReport.startListItem();
                String href = landscapeAnalysisResults.getConfiguration().getProjectReportsUrlPrefix() + subLandscape.getIndexFilePath();
                String label = getLabel(subLandscape);
                landscapeReport.addNewTabLink(label, href);
                landscapeReport.endListItem();
            });

            landscapeReport.endUnorderedList();

            landscapeReport.endSection();
        }

    }

    private String getLabel(SubLandscapeLink subLandscape) {
        return subLandscape.getIndexFilePath().replaceAll("(/|\\\\)_sokrates_landscape(/|\\\\).*", "");
    }

    private void addBigSummary(LandscapeAnalysisResults landscapeAnalysisResults) {
        landscapeReport.startDiv("margin-top: 0px;");
        LandscapeConfiguration configuration = landscapeAnalysisResults.getConfiguration();
        int thresholdContributors = configuration.getProjectThresholdContributors();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(getProjects().size()), "projects",
                "", "active project with " + (thresholdContributors > 1 ? "(" + thresholdContributors + "+&nbsp;contributors)" : ""));
        int extensionsCount = getLinesOfCodePerExtension().size();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(landscapeAnalysisResults.getMainLoc()), "lines of code (main)", "", getExtraLocInfo());
        int mainLocActive = landscapeAnalysisResults.getMainLocActive();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(mainLocActive), "lines of code (active)", "", "files updated in past year");
        int mainLocNew = landscapeAnalysisResults.getMainLocNew();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(mainLocNew), "lines of code (new)", "", "files created in past year");

        List<ContributorProjects> contributors = landscapeAnalysisResults.getContributors();
        long contributorsCount = contributors.size();
        if (contributorsCount > 0) {
            int recentContributorsCount = landscapeAnalysisResults.getRecentContributorsCount();
            int locPerRecentContributor = 0;
            int locNewPerRecentContributor = 0;
            if (recentContributorsCount > 0) {
                locPerRecentContributor = (int) Math.round((double) mainLocActive / recentContributorsCount);
                locNewPerRecentContributor = (int) Math.round((double) mainLocNew / recentContributorsCount);
            }
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(recentContributorsCount), "recent contributors",
                    "(past 30 days)", getExtraPeopleInfo(contributors, contributorsCount) + "\n" + FormattingUtils.getFormattedCount(locPerRecentContributor) + " active lines of code per recent contributor");
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(landscapeAnalysisResults.getRookiesContributorsCount()), "active rookies",
                    "(started in past year)", "active contributors with the first commit in past year");
        }

        addContributorsPerYear(false);

        landscapeReport.addLineBreak();

        if (configuration.getCustomMetrics().size() > 0) {
            configuration.getCustomMetrics().forEach(customMetric -> addCustomInfoBlock(customMetric));
            landscapeReport.addLineBreak();
        }

        if (configuration.getCustomMetricsSmall().size() > 0) {
            configuration.getCustomMetricsSmall().forEach(customMetric -> {
                addSmallInfoBlock(customMetric.getValue(), customMetric.getTitle(), customMetric.getColor(), customMetric.getLink());
            });
        }

        landscapeReport.endDiv();
        landscapeReport.addLineBreak();

        addIFrames(configuration);
        addCustomTags(configuration);
    }

    private void addBigProjectsSummary(LandscapeAnalysisResults landscapeAnalysisResults) {
        LandscapeConfiguration configuration = landscapeAnalysisResults.getConfiguration();
        int thresholdContributors = configuration.getProjectThresholdContributors();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(getProjects().size()), "projects",
                "", "active project with " + (thresholdContributors > 1 ? "(" + thresholdContributors + "+&nbsp;contributors)" : ""));
        int extensionsCount = getLinesOfCodePerExtension().size();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(landscapeAnalysisResults.getMainLoc()), "lines of code (main)", "", getExtraLocInfo());
        int mainLocActive = landscapeAnalysisResults.getMainLocActive();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(mainLocActive), "lines of code (active)", "", "files updated in past year");
        int mainLocNew = landscapeAnalysisResults.getMainLocNew();
        addInfoBlock(FormattingUtils.getSmallTextForNumber(mainLocNew), "lines of code (new)", "", "files created in past year");
    }

    private void addBigContributorsSummary(LandscapeAnalysisResults landscapeAnalysisResults) {
        List<ContributorProjects> contributors = landscapeAnalysisResults.getContributors();
        long contributorsCount = contributors.size();
        int mainLocActive = landscapeAnalysisResults.getMainLocActive();
        int mainLocNew = landscapeAnalysisResults.getMainLocNew();
        if (contributorsCount > 0) {
            int recentContributorsCount = landscapeAnalysisResults.getRecentContributorsCount();
            int locPerRecentContributor = 0;
            int locNewPerRecentContributor = 0;
            if (recentContributorsCount > 0) {
                locPerRecentContributor = (int) Math.round((double) mainLocActive / recentContributorsCount);
                locNewPerRecentContributor = (int) Math.round((double) mainLocNew / recentContributorsCount);
            }
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(recentContributorsCount), "recent contributors",
                    "(past 30 days)", getExtraPeopleInfo(contributors, contributorsCount) + "\n" + FormattingUtils.getFormattedCount(locPerRecentContributor) + " active lines of code per recent contributor");
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(landscapeAnalysisResults.getRookiesContributorsCount()), "active rookies",
                    "(started in past year)", "active contributors with the first commit in past year");
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(locPerRecentContributor), "contributor load",
                    "(active LOC/contributor)", "active lines of code per recent contributor\n\n" + FormattingUtils.getPlainTextForNumber(locNewPerRecentContributor) + " new LOC/recent contributor");
            List<ComponentDependency> peopleDependencies = this.getPeopleDependencies(30);
            peopleDependencies.sort((a, b) -> b.getCount() - a.getCount());
            List<ContributorConnections> names = names(peopleDependencies, 30);
            int cIndex = getCIndex(names);
            int pIndex = getPIndex(names);
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(cIndex), "C-Index",
                    "30 days", "You have " + cIndex + " active contributes connected to " + cIndex + " or more of other contributers via commits to shared projects in past 30 days.");
            addPeopleInfoBlock(FormattingUtils.getSmallTextForNumber(pIndex), "P-Index",
                    "30 days", "You have " + pIndex + " active contributes committing to " + pIndex + " or more of projects in past 30 days.");
        }

        addContributorsPerYear(true);
    }

    private void addIFrames(LandscapeConfiguration configuration) {
        if (configuration.getiFrames().size() > 0) {
            configuration.getiFrames().forEach(iframe -> {
                if (StringUtils.isNotBlank(iframe.getTitle())) {
                    String title;
                    if (StringUtils.isNotBlank(iframe.getMoreInfoLink())) {
                        title = "<a href='" + iframe.getMoreInfoLink() + "' target='_blank' style='text-decoration: none'>" + iframe.getTitle() + "</a>";
                    } else {
                        title = iframe.getTitle();
                    }
                    landscapeReport.startSubSection(title, "");
                }
                String style = StringUtils.defaultIfBlank(iframe.getStyle(), "width: 100%; height: 200px; border: 1px solid lightgrey;");
                landscapeReport.addHtmlContent("<iframe src='" + iframe.getSrc()
                        + "' frameborder='0' style='" + style + "'"
                        + (iframe.getScrolling() ? "" : " scrolling='no' ")
                        + "></iframe>");
                if (StringUtils.isNotBlank(iframe.getTitle())) {
                    landscapeReport.endSection();
                }
            });
        }
    }

    private void addCustomTags(LandscapeConfiguration configuration) {
        if (configuration.getTags().getGroups().size() > 0) {
            landscapeReport.startDiv("margin-bottom: 20px");
            configuration.getTags().getGroups().forEach(tagGroup -> {
                renderTagGroup(configuration, tagGroup);
            });
            landscapeReport.endDiv();
        }
    }

    private void renderTagGroup(LandscapeConfiguration configuration, CustomTagGroup tagGroup) {
        if (anyTagsPresent(tagGroup)) {
            landscapeReport.startDiv("display: inline-block; border: 1px solid lightgrey; padding: 6px; border-radius: 6px;");
            landscapeReport.addParagraph(tagGroup.getName(), "font-size: 70%; color: grey;");
            tagGroup.getTags().forEach(tag -> {
                renderTag(configuration, tag);
            });
            tagGroup.getSubGroups().forEach(subGroup -> {
                renderTagGroup(configuration, subGroup);
            });
            landscapeReport.endDiv();
        }
    }

    private boolean anyTagsPresent(CustomTagGroup tagGroup) {
        if (tagGroup.getTags().size() > 0) {
            return true;
        }

        for (CustomTagGroup subGroup : tagGroup.getSubGroups()) {
            if (anyTagsPresent(subGroup)) {
                return true;
            }
        }

        return false;
    }

    private void renderTag(LandscapeConfiguration configuration, CustomTag tag) {
        String logoSrc = configuration.getTags().getLogosRoot() + tag.getIcon();
        landscapeReport.startSpan("position: relative;");
        String imageStyle = "width: 80px; height: 60px; object-fit: contain;";
        String title = tag.getTitle();
        if (StringUtils.isNotBlank(tag.getDescription())) {
            title += "\n\n" + tag.getDescription();
        }
        if (StringUtils.isNotBlank(tag.getLink())) {
            landscapeReport.addHtmlContent("<a target='_blank' href='" + tag.getLink() + "'>");
        }
        landscapeReport.addHtmlContent("<img src='" + logoSrc + "' title='" + title + "' style='" + imageStyle + "'>");
        if (StringUtils.isNotBlank(tag.getMark())) {
            landscapeReport.addHtmlContent("<span style='border: 1px solid lightgrey; font-size: 80%; background-color: yellow; position: absolute; top: -44px; left: 0px;'>&nbsp;" + tag.getMark() + "&nbsp;</span>");
        }
        if (StringUtils.isNotBlank(tag.getLink())) {
            landscapeReport.addHtmlContent("</a>");
        }
        landscapeReport.endSpan();
    }

    private void addExtensions() {
        int threshold = landscapeAnalysisResults.getConfiguration().getExtensionThresholdLoc();

        List<NumericMetric> linesOfCodePerExtension = getLinesOfCodePerExtension();
        landscapeReport.startSubSection("File Extensions in Main Code (" + linesOfCodePerExtension.size() + ")",
                threshold >= 1 ? threshold + "+ lines of code" : "");
        landscapeReport.startDiv("");
        landscapeReport.addNewTabLink("bubble chart", "visuals/bubble_chart_extensions.html");
        landscapeReport.addHtmlContent(" | ");
        landscapeReport.addNewTabLink("tree map", "visuals/tree_map_extensions.html");
        landscapeReport.addLineBreak();
        landscapeReport.addLineBreak();
        landscapeReport.endDiv();
        landscapeReport.startDiv("");
        boolean tooLong = linesOfCodePerExtension.size() > 25;
        List<NumericMetric> linesOfCodePerExtensionDisplay = tooLong ? linesOfCodePerExtension.subList(0, 25) : linesOfCodePerExtension;
        List<NumericMetric> linesOfCodePerExtensionHide = tooLong ? linesOfCodePerExtension.subList(25, linesOfCodePerExtension.size()) : new ArrayList<>();
        linesOfCodePerExtensionDisplay.forEach(extension -> {
            String smallTextForNumber = FormattingUtils.getSmallTextForNumber(extension.getValue().intValue());
            addSmallInfoBlockLoc(smallTextForNumber, extension.getName().replace("*.", ""), null);
        });
        if (linesOfCodePerExtensionHide.size() > 0) {
            landscapeReport.startShowMoreBlockDisappear("", "show all...");
            linesOfCodePerExtensionHide.forEach(extension -> {
                String smallTextForNumber = FormattingUtils.getSmallTextForNumber(extension.getValue().intValue());
                addSmallInfoBlockLoc(smallTextForNumber, extension.getName().replace("*.", ""), null);
            });
            landscapeReport.endShowMoreBlock();
        }
        landscapeReport.endDiv();
        landscapeReport.endSection();
    }

    private List<NumericMetric> getLinesOfCodePerExtension() {
        int threshold = landscapeAnalysisResults.getConfiguration().getExtensionThresholdLoc();
        return landscapeAnalysisResults.getLinesOfCodePerExtension().stream()
                .filter(e -> !e.getName().endsWith("="))
                .filter(e -> !e.getName().startsWith("h-"))
                .filter(e -> e.getValue().intValue() >= threshold)
                .collect(Collectors.toCollection(ArrayList::new));
    }

    private void addContributors() {
        List<ContributorProjects> contributors = landscapeAnalysisResults.getContributors();
        int contributorsCount = landscapeAnalysisResults.getContributorsCount();

        if (contributorsCount > 0) {
            int thresholdCommits = landscapeAnalysisResults.getConfiguration().getContributorThresholdCommits();
            int totalCommits = contributors.stream().mapToInt(c -> c.getContributor().getCommitsCount()).sum();
            final String[] latestCommit = {""};
            contributors.forEach(c -> {
                if (c.getContributor().getLatestCommitDate().compareTo(latestCommit[0]) > 0) {
                    latestCommit[0] = c.getContributor().getLatestCommitDate();
                }
            });

            landscapeReport.startSubSection("Contributors (" + contributorsCount + ")",
                    (thresholdCommits > 1 ? thresholdCommits + "+&nbsp;commits, " : "") + "latest commit " + latestCommit[0]);

            addContributorLinks();

            if (contributorsCount > 100) {
                landscapeReport.startShowMoreBlock("show details...");
            }
            landscapeReport.startTable("width: 100%");
            landscapeReport.addTableHeader("", "Contributor", "# commits", "# commits<br>30 days", "# commits<br>90 days", "first", "latest", "projects");

            int counter[] = {0};

            contributors.forEach(contributor -> {
                addContributor(totalCommits, counter, contributor);
            });
            landscapeReport.endTable();
            if (contributorsCount > 100) {
                landscapeReport.endShowMoreBlock();
            }
            landscapeReport.endSection();
        }

    }

    private void addContributorsPerExtension() {
        int commitsCount = landscapeAnalysisResults.getCommitsCount();
        if (commitsCount > 0) {
            List<CommitsPerExtension> perExtension = landscapeAnalysisResults.getContributorsPerExtension();

            if (perExtension.size() > 0) {
                landscapeReport.startSubSection("Commits & File Extensions (" + perExtension.size() + ")", "");

                if (perExtension.size() > 100) {
                    landscapeReport.startShowMoreBlock("show details...");
                }
                landscapeReport.startTable("width: 100%");
                landscapeReport.addTableHeader("Extension",
                        "# contributors<br>30 days", "# commits<br>30 days", "# files<br>30 days",
                        "# contributors<br>90 days", "# commits<br>90 days", "# files<br>90 days",
                        "# contributors", "# commits", "# files");

                perExtension.forEach(commitsPerExtension -> {
                    addCommitExtension(commitsPerExtension);
                });
                landscapeReport.endTable();
                if (perExtension.size() > 100) {
                    landscapeReport.endShowMoreBlock();
                }

                landscapeReport.endSection();
            }
        }
    }

    private void addContributorLinks() {
        landscapeReport.addNewTabLink("bubble chart", "visuals/bubble_chart_contributors.html");
        landscapeReport.addHtmlContent(" | ");
        landscapeReport.addNewTabLink("tree map", "visuals/tree_map_contributors.html");
        landscapeReport.addHtmlContent(" | ");
        landscapeReport.addNewTabLink("data", "data/contributors.txt");
        landscapeReport.addLineBreak();
        landscapeReport.addLineBreak();
    }

    private void addContributor(int totalCommits, int[] counter, ContributorProjects contributor) {
        landscapeReport.startTableRow(contributor.getContributor().isActive(RECENT_THRESHOLD_DAYS) ? "font-weight: bold;"
                : "color: " + (contributor.getContributor().isActive(90) ? "grey" : "lightgrey"));
        counter[0] += 1;
        landscapeReport.addTableCell("" + counter[0], "text-align: center; vertical-align: top; padding-top: 13px;");
        landscapeReport.addTableCell(StringEscapeUtils.escapeHtml4(contributor.getContributor().getEmail()), "vertical-align: top; padding-top: 13px;");
        int contributerCommits = contributor.getContributor().getCommitsCount();
        double percentage = 100.0 * contributerCommits / totalCommits;
        landscapeReport.addTableCell(contributerCommits + " (" + FormattingUtils.getFormattedPercentage(percentage) + "%)", "vertical-align: top; padding-top: 13px;");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(contributor.getContributor().getCommitsCount30Days()), "vertical-align: top; padding-top: 13px;");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(contributor.getContributor().getCommitsCount90Days()), "vertical-align: top; padding-top: 13px;");
        landscapeReport.addTableCell(contributor.getContributor().getFirstCommitDate(), "vertical-align: top; padding-top: 13px;");
        landscapeReport.addTableCell(contributor.getContributor().getLatestCommitDate(), "vertical-align: top; padding-top: 13px;");
        StringBuilder projectInfo = new StringBuilder();
        landscapeReport.startTableCell();
        int projectsCount = contributor.getProjects().size();
        landscapeReport.startShowMoreBlock(projectsCount + (projectsCount == 1 ? " project" : " projects"));
        contributor.getProjects().forEach(contributorProjectInfo -> {
            String projectName = contributorProjectInfo.getProjectAnalysisResults().getAnalysisResults().getMetadata().getName();
            int commits = contributorProjectInfo.getCommitsCount();
            if (projectInfo.length() > 0) {
                projectInfo.append("<br/>");
            }
            projectInfo.append(projectName + " <span style='color: grey'>(" + commits + (commits == 1 ? " commit" : " commit") + ")</span>");
        });
        landscapeReport.addHtmlContent(projectInfo.toString());
        landscapeReport.endTableCell();
        landscapeReport.endTableRow();
    }

    private void addCommitExtension(CommitsPerExtension commitsPerExtension) {
        landscapeReport.startTableRow(commitsPerExtension.getCommitters30Days().size() > 0 ? "font-weight: bold;"
                : "color: " + (commitsPerExtension.getCommitters90Days().size() > 0 ? "grey" : "lightgrey"));
        landscapeReport.addTableCell("" + commitsPerExtension.getExtension(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitters30Days().size(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitsCount30Days(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getFilesCount30Days(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitters90Days().size(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getFilesCount90Days(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitsCount90Days(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitters().size(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getCommitsCount(), "text-align: center;");
        landscapeReport.addTableCell("" + commitsPerExtension.getFilesCount(), "text-align: center;");
        landscapeReport.endTableCell();
        landscapeReport.endTableRow();
    }

    private void addProjectsSection(List<ProjectAnalysisResults> projectsAnalysisResults) {
        Collections.sort(projectsAnalysisResults, (a, b) -> b.getAnalysisResults().getMainAspectAnalysisResults().getLinesOfCode() - a.getAnalysisResults().getMainAspectAnalysisResults().getLinesOfCode());
        landscapeReport.startSubSection("Projects (" + projectsAnalysisResults.size() + ")", "");

        if (projectsAnalysisResults.size() > 0) {
            List<NumericMetric> projectSizes = new ArrayList<>();
            projectsAnalysisResults.forEach(projectAnalysisResults -> {
                CodeAnalysisResults analysisResults = projectAnalysisResults.getAnalysisResults();
                projectSizes.add(new NumericMetric(analysisResults.getMetadata().getName(), analysisResults.getMainAspectAnalysisResults().getLinesOfCode()));
            });
            landscapeReport.addNewTabLink("bubble chart", "visuals/bubble_chart_projects.html");
            landscapeReport.addHtmlContent(" | ");
            landscapeReport.addNewTabLink("tree map", "visuals/tree_map_projects.html");
            landscapeReport.addHtmlContent(" | ");
            landscapeReport.addNewTabLink("data", "data/projects.txt");
            landscapeReport.addLineBreak();
            landscapeReport.addLineBreak();
            if (projectsAnalysisResults.size() > 100) {
                landscapeReport.startShowMoreBlock("show details...");
            }
            landscapeReport.startTable("width: 100%");
            int thresholdCommits = landscapeAnalysisResults.getConfiguration().getContributorThresholdCommits();
            int thresholdContributors = landscapeAnalysisResults.getConfiguration().getProjectThresholdContributors();
            landscapeReport.addTableHeader("",
                    "Project" + (thresholdContributors > 1 ? "<br/>(" + thresholdContributors + "+&nbsp;contributors)" : ""),
                    "Main<br/>Language", "LOC<br/>(main)",
                    "LOC<br/>(test)", "LOC<br/>(other)",
                    "Age", "Contributors" + (thresholdCommits > 1 ? "<br/>(" + thresholdCommits + "+&nbsp;commits)" : ""),
                    "Recent<br>Contributors<br>(30d)", "Rookies", "Commits<br>this year", "Report");
            Collections.sort(projectsAnalysisResults,
                    (a, b) -> b.getAnalysisResults().getContributorsAnalysisResults().getCommitsThisYear()
                            - a.getAnalysisResults().getContributorsAnalysisResults().getCommitsThisYear());
            projectsAnalysisResults.forEach(projectAnalysis -> {
                addProjectRow(projectAnalysis);
            });
            landscapeReport.endTable();
            if (projectsAnalysisResults.size() > 100) {
                landscapeReport.endShowMoreBlock();
            }
        }

        landscapeReport.endSection();
    }

    private void addProjectRow(ProjectAnalysisResults projectAnalysis) {
        CodeAnalysisResults analysisResults = projectAnalysis.getAnalysisResults();
        Metadata metadata = analysisResults.getMetadata();
        String logoLink = metadata.getLogoLink();

        landscapeReport.startTableRow();
        landscapeReport.addTableCell(StringUtils.isNotBlank(logoLink) ? "<img src='" + logoLink + "' style='width: 20px'>" : "", "text-align: center");
        landscapeReport.addTableCell(metadata.getName());
        AspectAnalysisResults main = analysisResults.getMainAspectAnalysisResults();
        AspectAnalysisResults test = analysisResults.getTestAspectAnalysisResults();
        AspectAnalysisResults generated = analysisResults.getGeneratedAspectAnalysisResults();
        AspectAnalysisResults build = analysisResults.getBuildAndDeployAspectAnalysisResults();
        AspectAnalysisResults other = analysisResults.getOtherAspectAnalysisResults();

        int thresholdCommits = landscapeAnalysisResults.getConfiguration().getContributorThresholdCommits();
        List<Contributor> contributors = analysisResults.getContributorsAnalysisResults().getContributors()
                .stream().filter(c -> c.getCommitsCount() >= thresholdCommits).collect(Collectors.toCollection(ArrayList::new));

        int contributorsCount = contributors.size();
        int recentContributorsCount = (int) contributors.stream().filter(c -> c.isActive(RECENT_THRESHOLD_DAYS)).count();
        int rookiesCount = (int) contributors.stream().filter(c -> c.isRookie(RECENT_THRESHOLD_DAYS)).count();

        List<NumericMetric> linesOfCodePerExtension = main.getLinesOfCodePerExtension();
        StringBuilder locSummary = new StringBuilder();
        if (linesOfCodePerExtension.size() > 0) {
            locSummary.append(linesOfCodePerExtension.get(0).getName().replace("*.", "").trim().toUpperCase());
        } else {
            locSummary.append("-");
        }
        landscapeReport.addTableCell(locSummary.toString().replace("> = ", ">"), "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(main.getLinesOfCode(), "-"), "text-align: center");

        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(test.getLinesOfCode(), "-"), "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(generated.getLinesOfCode() + build.getLinesOfCode() + other.getLinesOfCode(), "-"), "text-align: center");
        int projectAgeYears = (int) Math.round(analysisResults.getFilesHistoryAnalysisResults().getAgeInDays() / 365.0);
        String age = projectAgeYears == 0 ? "<1y" : projectAgeYears + "y";
        landscapeReport.addTableCell(age, "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(contributorsCount, "-"), "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(recentContributorsCount, "-"), "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(rookiesCount, "-"), "text-align: center");
        landscapeReport.addTableCell(FormattingUtils.getFormattedCount(analysisResults.getContributorsAnalysisResults().getCommitsThisYear(), "-"), "text-align: center");
        String projectReportUrl = landscapeAnalysisResults.getConfiguration().getProjectReportsUrlPrefix() + projectAnalysis.getSokratesProjectLink().getHtmlReportsRoot() + "/index.html";
        landscapeReport.addTableCell("<a href='" + projectReportUrl + "' target='_blank'>"
                + "<div style='height: 40px'>" + ReportFileExporter.getIconSvg("report", 40) + "</div></a>", "text-align: center");
        landscapeReport.endTableRow();
    }

    private void addCustomInfoBlock(CustomMetric customMetric) {
        String subtitle = customMetric.getTitle();
        if (StringUtils.isNotBlank(customMetric.getSubTitle())) {
            subtitle += "<br/><span style='color: grey; font-size: 80%'>" + customMetric.getSubTitle() + "</span>";
        }
        String color = StringUtils.isNotBlank(customMetric.getColor()) ? customMetric.getColor() : "lightgrey";
        addInfoBlockWithColor(customMetric.getValue(), subtitle, color, "");
    }

    private void addInfoBlock(String mainValue, String subtitle, String description, String tooltip) {
        if (StringUtils.isNotBlank(description)) {
            subtitle += "<br/><span style='color: grey; font-size: 80%'>" + description + "</span>";
        }
        addInfoBlockWithColor(mainValue, subtitle, "skyblue", tooltip);
    }

    private String getExtraLocInfo() {
        String info = "";

        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getMainLoc()) + " LOC (main)\n";
        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getTestLoc()) + " LOC (test)\n";
        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getGeneratedLoc()) + " LOC (generated)\n";
        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getBuildAndDeploymentLoc()) + " LOC (build and deployment)\n";
        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getOtherLoc()) + " LOC (other)";

        return info;
    }

    private String getExtraPeopleInfo(List<ContributorProjects> contributors, long contributorsCount) {
        String info = "";

        int recentContributorsCount6Months = landscapeAnalysisResults.getRecentContributorsCount6Months();
        int recentContributorsCount3Months = landscapeAnalysisResults.getRecentContributorsCount3Months();
        info += FormattingUtils.getPlainTextForNumber(landscapeAnalysisResults.getRecentContributorsCount()) + " contributors (30 days)\n";
        info += FormattingUtils.getPlainTextForNumber(recentContributorsCount3Months) + " contributors (3 months)\n";
        info += FormattingUtils.getPlainTextForNumber(recentContributorsCount6Months) + " contributors (6 months)\n";

        LandscapeConfiguration configuration = landscapeAnalysisResults.getConfiguration();
        int thresholdCommits = configuration.getContributorThresholdCommits();
        info += FormattingUtils.getPlainTextForNumber((int) contributorsCount) + " contributors (all time)\n";
        info += "\nOnly the contributors with " + (thresholdCommits > 1 ? "(" + thresholdCommits + "+&nbsp;commits)" : "") + " included";

        return info;
    }

    private void addPeopleInfoBlock(String mainValue, String subtitle, String description, String tooltip) {
        if (StringUtils.isNotBlank(description)) {
            subtitle += "<br/><span style='color: grey; font-size: 80%'>" + description + "</span>";
        }
        addInfoBlockWithColor(mainValue, subtitle, "lavender", tooltip);
    }

    private void addCommitsInfoBlock(String mainValue, String subtitle, String description, String tooltip) {
        if (StringUtils.isNotBlank(description)) {
            subtitle += "<br/><span style='color: grey; font-size: 80%'>" + description + "</span>";
        }
        addInfoBlockWithColor(mainValue, subtitle, "#fefefe", tooltip);
    }

    private void addInfoBlockWithColor(String mainValue, String subtitle, String color, String tooltip) {
        String style = "border-radius: 12px;";

        style += "margin: 12px 12px 12px 0px;";
        style += "display: inline-block; width: 160px; height: 120px;";
        style += "background-color: " + color + "; text-align: center; vertical-align: middle; margin-bottom: 36px;";

        landscapeReport.startDiv(style, tooltip);
        landscapeReport.addHtmlContent("<div style='font-size: 50px; margin-top: 20px'>" + mainValue + "</div>");
        landscapeReport.addHtmlContent("<div style='color: #434343; font-size: 16px'>" + subtitle + "</div>");
        landscapeReport.endDiv();
    }

    private void addSmallInfoBlockLoc(String value, String subtitle, String link) {
        addSmallInfoBlock(value, subtitle, "skyblue", link);
    }

    private void addSmallInfoBlockPeople(String value, String subtitle, String link) {
        addSmallInfoBlock(value, subtitle, "lavender", link);
    }

    private void addSmallInfoBlock(String value, String subtitle, String color, String link) {
        String style = "border-radius: 8px;";

        style += "margin: 4px 4px 4px 0px;";
        style += "display: inline-block; width: 80px; height: 76px;";
        style += "background-color: " + color + "; text-align: center; vertical-align: middle; margin-bottom: 16px;";

        landscapeReport.startDiv(style);
        if (StringUtils.isNotBlank(link)) {
            landscapeReport.startNewTabLink(link, "text-decoration: none");
        }
        landscapeReport.addHtmlContent("<div style='font-size: 24px; margin-top: 8px;'>" + value + "</div>");
        landscapeReport.addHtmlContent("<div style='color: #434343; font-size: 13px'>" + subtitle + "</div>");
        if (StringUtils.isNotBlank(link)) {
            landscapeReport.endNewTabLink();
        }
        landscapeReport.endDiv();
    }

    public List<RichTextReport> report() {
        List<RichTextReport> reports = new ArrayList<>();

        reports.add(this.landscapeReport);

        return reports;
    }

    private void addContributorsPerYear(boolean showContributorsCount) {
        List<ContributionYear> contributorsPerYear = landscapeAnalysisResults.getContributorsPerYear();
        if (contributorsPerYear.size() > 0) {
            int limit = 20;
            if (contributorsPerYear.size() > limit) {
                contributorsPerYear = contributorsPerYear.subList(contributorsPerYear.size() - limit, contributorsPerYear.size());
            }

            int maxCommits = contributorsPerYear.stream().mapToInt(c -> c.getCommitsCount()).max().orElse(1);

            landscapeReport.startTable();

            landscapeReport.startTableRow();
            landscapeReport.startTableCell("border: none; height: 100px");
            int commitsCount = landscapeAnalysisResults.getCommitsCount();
            if (commitsCount > 0) {
                landscapeReport.startDiv("max-height: 105px");
                addSmallInfoBlock(FormattingUtils.getSmallTextForNumber(commitsCount), "commits", "white", "");
                landscapeReport.endDiv();
            }
            landscapeReport.endTableCell();
            String style = "border: none; text-align: center; vertical-align: bottom; font-size: 80%; height: 100px";
            contributorsPerYear.forEach(year -> {
                landscapeReport.startTableCell(style);
                int count = year.getCommitsCount();
                landscapeReport.addParagraph(count + "", "margin: 2px");
                int height = 1 + (int) (64.0 * count / maxCommits);
                landscapeReport.addHtmlContent("<div style='width: 100%; background-color: darkgrey; height:" + height + "px'></div>");
                landscapeReport.endTableCell();
            });
            landscapeReport.endTableRow();

            if (showContributorsCount) {
                int maxContributors[] = {1};
                contributorsPerYear.forEach(year -> {
                    int count = getContributorsCountPerYear(year.getYear());
                    maxContributors[0] = Math.max(maxContributors[0], count);
                });
                landscapeReport.startTableRow();
                landscapeReport.startTableCell("border: none; height: 100px");
                int contributorsCount = landscapeAnalysisResults.getContributors().size();
                if (contributorsCount > 0) {
                    landscapeReport.startDiv("max-height: 105px");
                    addSmallInfoBlock(FormattingUtils.getSmallTextForNumber(contributorsCount), "contributors", "white", "");
                    landscapeReport.endDiv();
                }
                landscapeReport.endTableCell();
                contributorsPerYear.forEach(year -> {
                    landscapeReport.startTableCell(style);
                    int count = getContributorsCountPerYear(year.getYear());
                    landscapeReport.addParagraph(count + "", "margin: 2px");
                    int height = 1 + (int) (64.0 * count / maxContributors[0]);
                    landscapeReport.addHtmlContent("<div style='width: 100%; background-color: skyblue; height:" + height + "px'></div>");
                    landscapeReport.endTableCell();
                });
                landscapeReport.endTableRow();
            }

            landscapeReport.startTableRow();
            landscapeReport.addTableCell("", "border: none; ");
            contributorsPerYear.forEach(year -> {
                landscapeReport.addTableCell(year.getYear(), "border: none; text-align: center; font-size: 90%");
            });
            landscapeReport.endTableRow();

            landscapeReport.endTable();

            landscapeReport.addLineBreak();
        }
    }

    private int getContributorsCountPerYear(String year) {
        int count[] = {0};

        landscapeAnalysisResults.getContributors().forEach(contributorProjects -> {
            if (contributorProjects.getContributor().getActiveYears().contains(year)) {
                count[0] += 1;
            }
        });

        return count[0];
    }

    private List<ComponentDependency> getPeopleDependencies(int daysAgo) {
        Map<String, List<String>> projectsMap = new HashMap<>();

        landscapeAnalysisResults.getContributors().stream()
                .forEach(contributorProjects -> {
                    contributorProjects.getProjects().stream()
                            .filter(project -> DateUtils.isCommittedLessThanDaysAgo(project.getLatestCommitDate(), daysAgo))
                            .forEach(project -> {
                                String email = contributorProjects.getContributor().getEmail();
                                String projectName = project.getProjectAnalysisResults().getAnalysisResults().getMetadata().getName();
                                if (projectsMap.containsKey(projectName)) {
                                    List<String> emails = projectsMap.get(projectName);
                                    if (!emails.contains(email)) {
                                        emails.add(email);
                                    }
                                } else {
                                    projectsMap.put(projectName, new ArrayList<>(Arrays.asList(email)));
                                }
                            });
                });

        List<ComponentDependency> dependencies = new ArrayList<>();
        Map<String, ComponentDependency> dependenciesMap = new HashMap<>();
        Map<String, List<String>> projectNamesMap = new HashMap<>();

        projectsMap.keySet().forEach(projectName -> {
            List<String> emails = projectsMap.get(projectName);
            emails.forEach(email1 -> {
                emails.stream().filter(email2 -> !email1.equalsIgnoreCase(email2)).forEach(email2 -> {
                    String key1 = email1 + "::" + email2;
                    String key2 = email2 + "::" + email1;

                    if (dependenciesMap.containsKey(key1)) {
                        if (!projectNamesMap.get(key1).contains(projectName)) {
                            dependenciesMap.get(key1).increment(1);
                            projectNamesMap.get(key1).add(projectName);
                        }
                    } else if (dependenciesMap.containsKey(key2)) {
                        if (!projectNamesMap.get(key2).contains(projectName)) {
                            dependenciesMap.get(key2).increment(1);
                            projectNamesMap.get(key2).add(projectName);
                        }
                    } else {
                        ComponentDependency dependency = new ComponentDependency(email1, email2);
                        dependenciesMap.put(key1, dependency);
                        dependencies.add(dependency);
                        projectNamesMap.put(key1, new ArrayList<>(Arrays.asList(projectName)));
                    }
                });
            });
        });

        return dependencies;
    }

    private int getProjectCount(String email, int daysAgo) {
        Set<String> projectNames = new HashSet<>();
        landscapeAnalysisResults.getContributors().stream()
                .filter(c -> c.getContributor().getEmail().equalsIgnoreCase(email))
                .forEach(contributorProjects -> {
                    List<ContributorProjectInfo> projects = contributorProjects.getProjects();
                    projects.stream()
                            .filter(p -> DateUtils.isCommittedLessThanDaysAgo(p.getLatestCommitDate(), daysAgo))
                            .forEach(project -> {
                                projectNames.add(project.getProjectAnalysisResults().getAnalysisResults().getMetadata().getName());
                            });
                });

        return projectNames.size();
    }

    private void renderPeopleDependencies(int daysAgo) {
        landscapeReport.addLevel2Header("People Dependencies (" + daysAgo + " days)", "margin-top: 40px");
        landscapeReport.addParagraph("The number of same repositories that two persons committed to in the past " + daysAgo + " days.", "color: grey");
        GraphvizDependencyRenderer graphvizDependencyRenderer = new GraphvizDependencyRenderer();
        graphvizDependencyRenderer.setMaxNumberOfDependencies(200);
        graphvizDependencyRenderer.setType("graph");
        graphvizDependencyRenderer.setArrow("--");

        List<ComponentDependency> peopleDependencies = this.getPeopleDependencies(daysAgo);
        peopleDependencies.sort((a, b) -> b.getCount() - a.getCount());
        List<ContributorConnections> names = names(peopleDependencies, daysAgo);

        int cIndex = getCIndex(names);
        landscapeReport.addParagraph("C-index: <b>" + cIndex + "</b> <span style='color: grey'>You have " + cIndex + " people with " + cIndex + " or more project connections with other people.)</span>.");
        int pIndex = getPIndex(names);
        landscapeReport.addParagraph("P-index: <b>" + pIndex + "</b> <span style='color: grey'>(You have " + pIndex + " people commiting to " + pIndex + " or more projects.)</span>.");
        landscapeReport.startShowMoreBlock("show most connected people...<br>");
        landscapeReport.startTable();
        int index[] = {0};
        names.subList(0, Math.min(500000, names.size())).forEach(name -> {
            index[0] += 1;
            landscapeReport.startTableRow();
            landscapeReport.addTableCell(index[0] + "", "");
            landscapeReport.addTableCell(name.email, "");
            landscapeReport.addTableCell(name.projectsCount + "&nbsp;projects");
            landscapeReport.addTableCell(name.connectionsCount + " connections", "");
            landscapeReport.endTableRow();
        });
        landscapeReport.endTable();
        landscapeReport.endShowMoreBlock();

        landscapeReport.startShowMoreBlock("show top connections...<br>");
        landscapeReport.startTable();
        peopleDependencies.subList(0, Math.min(50, peopleDependencies.size())).forEach(dependency -> {
            landscapeReport.startTableRow();
            String from = dependency.getFromComponent();
            String to = dependency.getToComponent();
            landscapeReport.addTableCell(from + "<br><span style='color: grey'>" + getProjectCount(from, daysAgo) + " projects</span>", "");
            landscapeReport.addTableCell(to + "<br><span style='color: grey'>" + getProjectCount(to, daysAgo) + " projects</span>", "");
            landscapeReport.addTableCell(dependency.getCount() + " shared projects", "");
            landscapeReport.endTableRow();
        });
        landscapeReport.endTable();
        landscapeReport.endShowMoreBlock();


        landscapeReport.startShowMoreBlock("show graph...<br>");
        Set<String> emails = new HashSet<>();
        peopleDependencies.forEach(peopleDependency -> {
            emails.add(peopleDependency.getFromComponent());
            emails.add(peopleDependency.getToComponent());
        });

        String prefix = "people_dependencies_" + daysAgo + "_";
        addDependencyGraphVisuals(peopleDependencies, new ArrayList<>(), graphvizDependencyRenderer, prefix);
        landscapeReport.endShowMoreBlock();
    }

    private int getCIndex(List<ContributorConnections> names) {
        List<ContributorConnections> list = new ArrayList<>(names);
        list.sort((a, b) -> b.connectionsCount - a.connectionsCount);
        for (int factor = 0; factor < list.size(); factor++) {
            if (factor == list.get(factor).connectionsCount) {
                return factor;
            } else if (factor > list.get(factor).connectionsCount) {
                return factor - 1;
            }
        }
        return 0;
    }

    private int getPIndex(List<ContributorConnections> names) {
        List<ContributorConnections> list = new ArrayList<>(names);
        list.sort((a, b) -> b.projectsCount - a.projectsCount);
        for (int factor = 0; factor < list.size(); factor++) {
            if (factor == list.get(factor).projectsCount) {
                return factor;
            } else if (factor > list.get(factor).projectsCount) {
                return factor - 1;
            }
        }
        return 0;
    }

    private List<ContributorConnections> names(List<ComponentDependency> peopleDependencies, int daysAgo) {
        Map<String, ContributorConnections> map = new HashMap<>();

        peopleDependencies.forEach(dependency -> {
            String from = dependency.getFromComponent();
            String to = dependency.getToComponent();

            ContributorConnections contributorConnections1 = map.get(from);
            ContributorConnections contributorConnections2 = map.get(to);

            if (contributorConnections1 == null) {
                contributorConnections1 = new ContributorConnections();
                contributorConnections1.email = from;
                contributorConnections1.projectsCount = getProjectCount(from, daysAgo);
                contributorConnections1.connectionsCount = 1;
                map.put(from, contributorConnections1);
            } else {
                contributorConnections1.connectionsCount += 1;
            }

            if (contributorConnections2 == null) {
                contributorConnections2 = new ContributorConnections();
                contributorConnections2.email = to;
                contributorConnections2.projectsCount = getProjectCount(to, daysAgo);
                contributorConnections2.connectionsCount = 1;
                map.put(to, contributorConnections2);
            } else {
                contributorConnections2.connectionsCount += 1;
            }
        });

        List<ContributorConnections> names = new ArrayList<>(map.values());
        names.sort((a, b) -> b.connectionsCount - a.connectionsCount);
        // names.sort((a, b) -> b.projectsCount - a.projectsCount);

        return names;
    }

    private void addDependencyGraphVisuals(List<ComponentDependency> componentDependencies, List<String> componentNames, GraphvizDependencyRenderer graphvizDependencyRenderer, String prefix) {
        String graphvizContent = graphvizDependencyRenderer.getGraphvizContent(
                componentNames,
                componentDependencies);
        String graphId = prefix + dependencyVisualCounter++;
        landscapeReport.addGraphvizFigure(graphId, "", graphvizContent);
        landscapeReport.addLineBreak();
        landscapeReport.addLineBreak();
        addDownloadLinks(graphId);
    }

    private void addDownloadLinks(String graphId) {
        landscapeReport.startDiv("");
        landscapeReport.addHtmlContent("Download: ");
        landscapeReport.addNewTabLink("SVG", "visuals/" + graphId + ".svg");
        landscapeReport.addHtmlContent(" ");
        landscapeReport.addNewTabLink("DOT", "visuals/" + graphId + ".dot.txt");
        landscapeReport.addHtmlContent(" ");
        landscapeReport.addNewTabLink("(open online Graphviz editor)", "https://www.zeljkoobrenovic.com/tools/graphviz/");
        landscapeReport.endDiv();
    }


    class ContributorConnections {
        String email = "";
        int projectsCount;
        int connectionsCount;
    }
}

