# Contributing to VoxelMotion Studio

Thank you for your interest in contributing to **VoxelMotion Studio**.

## Branch Policy & Pull Requests

- `main` is a **protected branch**: direct pushes, force-pushes, and branch deletions are disabled for non-maintainers.
- Anyone is welcome to **clone, download builds, fork the repository, and propose improvements via Pull Requests**.
- **Owner Review Required:** Every Pull Request is manually reviewed by the repository owner before it can be merged. Passing automated CI checks does not automatically merge changes.
- **Project Note:** VoxelMotion Studio was created with AI-assisted development; human review focuses on correctness, simplicity, and maintaining clean behavior.

## Development Setup

1. Fork and clone your fork locally.
2. Enable the repository Conventional Commits hook and template (Windows PowerShell):
   ```powershell
   .\scripts\setup_git.ps1
   ```
   Or manually on Unix/macOS:
   ```sh
   cp scripts/hooks/commit-msg .git/hooks/commit-msg
   chmod +x .git/hooks/commit-msg
   git config --local commit.template .gitmessage
   ```
3. Run the release test suite before opening a PR:
   ```sh
   cargo test --release
   ```

## Commit Style

All commits must follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <imperative summary>
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
