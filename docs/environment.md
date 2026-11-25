# Managed environment

The managed environment is where unattended agent runs are meant to happen: a known place with recorded folders and snapshots, so a bad run can be rolled back and cannot touch the rest of your machine.

Only the first part is built, and only from the command line: recorded folders, works, snapshots and rollback. **It does not isolate anything yet.** The isolated image is planned and not made.

It needs git.

## Recorded folders

You list the folders the environment may see. A work can only be cut from a repository inside one of them.

```
maestro --env-mount <folder>
maestro --env-unmount <folder>
```

## Works

A work is a git worktree on its own branch, `maestro/<name>`, kept in Maestro's data folder.

```
maestro --work-new <repository> <name>
maestro --work
maestro --work-remove <name>
```

- Your repository's own files are not touched.
- Changes you have not committed in your repository are not carried into the work.
- `--work` lists works.
- `--work-remove` deletes the worktree and keeps its branch, so nothing committed there is lost.

## Snapshots and rollback

```
maestro --work-snapshot <name> [note]
maestro --work-rollback <name> [number]
```

A snapshot records a work exactly as it is, including files not yet committed. A rollback puts it back: the branch returns to the commit it was on, changed files are restored and stray files are removed. Without a number, the rollback goes to the latest snapshot.

Files git is told to ignore are neither recorded nor removed.

A snapshot is of one work, not of a whole image. It is stored as a commit under `refs/maestro/` in your repository.

## The image

```
maestro --env
maestro --env-use <distro>
maestro --env-use none
maestro --env-plan
```

| System | Where works run today |
|---|---|
| Windows | On the machine itself, or inside a WSL distro you already have |
| macOS | On the machine itself |
| Linux | On the machine itself |

- **Windows with WSL.** `--env-use <distro>` points Maestro at a WSL distro, and every git command then runs inside it. `--env-use none` goes back to this machine. `--env` looks inside the distro and says which tools it found and whether it can see all your drives. An ordinary distro can, so this does not isolate anything.
- **`--env-plan`** prints how a dedicated WSL image that sees only the recorded folders would be made. Maestro does not make it, and the steps have not been run.
- **macOS and Linux.** A container image is planned for both and not built.

A worktree made on one side (Windows or the distro) cannot be read by git on the other, because git records absolute paths. Maestro therefore runs every git command on the side you chose. If you change side with `--env-use`, make new works there.

## What is still to do

Making the dedicated image and confirming it sees only the recorded folders, snapshots of the whole image, a container image for macOS and Linux, and a page for all of this in the window. See the [plan](plan.md#managed-environment).
