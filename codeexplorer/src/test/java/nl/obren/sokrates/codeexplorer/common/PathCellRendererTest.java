/*
 * Copyright (c) 2021 Željko Obrenović. All rights reserved.
 */

package nl.obren.sokrates.codeexplorer.common;

import org.junit.Test;

import java.io.File;

import static junit.framework.TestCase.assertEquals;

public class PathCellRendererTest {
    private static final String PATH_SEPARATOR = "/";

    @Test
    public void getPathPrefix() throws Exception {
        PathCellRenderer renderer = new PathCellRenderer();

        assertEquals("/root/a/b/c/", renderer.getPathPrefix(new File("/root/a/b/c/A.java")));
        assertEquals("a" + PATH_SEPARATOR, renderer.getPathPrefix(new File("a/A.java")));
        assertEquals("", renderer.getPathPrefix(new File("A.java")));
    }

}
