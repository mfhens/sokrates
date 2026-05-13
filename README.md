# Sokrates

Know your code! The unexamined code is not worth maintaining!

For details and examples visit the website [sokrates.dev](https://sokrates.dev).

* Sokrates is built by Željko Obrenović. It implements his "examined code" vision on how to approach understanding of complex source code bases, in a pragmatic and efficient way.
* Sokrates is a code spelunking tool, inspired by the grep, adding structure on top of regex source code searches.
* Sokrates generates a number of reports that can help you understand your code.
* Sokrates comes with both command line interface and interactive GUI code explorer.

### Prerequirements
* Java 23
* Maven

This repository includes a `mise.toml` pin for Temurin 23. If you use `mise`, run:

> mise install

### Build

> mise exec -- mvn clean install

or, if your shell is already using the pinned JDK:

> mvn clean install

The build will create two jar files:
* the command line interface in the cli/target folder
* the interactive explorer in the codeexplorer/target folder

### Experimental Rust core

The repository also contains an experimental Rust analysis core in `rust-core\`.

Run the Rust tests:

> Set-Location rust-core
>
> cargo test --quiet

The Rust CLI currently supports:

* `analyze` to emit canonical `RepositoryAnalysis` JSON
* `export-data` to emit Java-compatible data bundle files, including `analysisResults.json`, from an existing Sokrates config

### Docker

Build the docker image:
> docker build -t sokrates .

Run init command:
> docker run -v "$(pwd):/code" -w /code sokrates init
