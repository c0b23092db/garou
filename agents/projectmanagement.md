---
name: Project Management Approach
description: How to proceed with the Project
---

# Project Management Approach
Last Updata: 2026/03/16 17:00

## When to Use this skill
- How to proceed with the project

## Use Command
- Use for Git Management: git
- Use for Version Management: [jj](https://github.com/jj-vcs/jj)
- Use for Project Initialization: uv,cargo,deno
- Use for cat: [bat](https://github.com/sharkdp/bat)
- Use for ls: [lsd](https://github.com/lsd-rs/lsd)
- Use for File Search: [fd](https://github.com/sharkdp/fd)
- Use for Text Search: [ripgrep](https://github.com/BurntSushi/ripgrep)
- Use for Structured Diff (JSON/YAML/TOML/XML/INI/CSV): [diffx](https://github.com/kako-jun/diffx)
- Use for Make Markdown to Annotation comment list: [suppa](https://github.com/c0b23092db/suppa)

## Example
### 1. Create a new project directory
```bash
~> cargo init my-rust-project # Rust
~> uv init my-python-project # Python
~> zig init my-typescript-project # Zig
~> deno init my-typescript-project # Typescript
```
### 2. `jj git init`: initialize a new git repository with `git init`
### 3. `jj new -m "~~~"`: New commit with message "~~~" with `git commit -m "~~~"`
### 4. Advance the project...
#### Run jj
```bash
~> # Undo
~> jj undo
~> # Diff: Checking the changes with `git diff HEAD`
~> jj diff
~> # Change the commit message with "~~~" with `git commit --amend --only`
~> jj describe -m "~~~"
~> # Checking the status with `git status`
~> jj st
~> # Checking the commit history with `git log --oneline --graph`
~> jj log
```
#### Run Test
```bash
~> #Test Running
~> cargo check # Rust
~> uv run pytest # Python
~> deno test # Typescript
```
### 5. `jj git push`: Push the changes to the remote repository
