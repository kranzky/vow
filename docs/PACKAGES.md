# Vow packages

A Vow package is a git repository of `.vow` files, hosted anywhere `git clone` can
reach: GitHub, GitLab, your own server. There is no registry; this page lists packages
people have published, and anyone can add one.

## Using a package

```
vowc add shapes https://github.com/someone/vow-shapes v1.0.0
```

This pins the package to the commit that the tag `v1.0.0` names now, in `vow.pkg`
(created if there is none), and fetches it into `.vow/pkg/`. Then import its files by
path, from the repository's root:

```
import "@shapes/shapes.vow" as shapes
```

The name (`shapes` here) is yours to choose; the package does not declare it. A pin is
an exact commit, so a build does not change when someone moves a tag or pushes to a
branch. To move to a newer version, run `vowc add` again with the new ref. Commit
`vow.pkg`; after a fresh clone, `vowc fetch` fetches everything it lists, and the
packages those list in turn.

A package's code is checked and verified with your program, like any file of your own.
Nothing is trusted because it was published, and fetching runs nothing but `git`. A
package can still read files or run programs if its functions declare those effects,
so read the `uses` clauses of what you call.

## Publishing a package

1. Put your `.vow` files in a git repository. Files that only the package uses can
   mark their helpers `private`.
2. If the package uses other packages, add them with `vowc add` in the repository's
   root, and commit the `vow.pkg` it writes (but not `.vow/`).
3. Check that every file verifies: `vowc check file.vow`.
4. Tag a release (`git tag v1.0.0`) and push the tag, so users can pin it by name.

Say in the README which files to import and the name you suggest for the package.

## Published packages

| Package | What it does | Add it with |
| --- | --- | --- |
| *(none yet)* | | |

## Getting a package listed

Open a pull request on [github.com/kranzky/vow](https://github.com/kranzky/vow) that
adds a row to the table above in `docs/PACKAGES.md`, keeping the table in alphabetical
order. A listed package:

- verifies with the current release of `vowc`;
- has a tagged release, used in the "Add it with" command;
- states its license in the repository.

A listing is not a review: read a package's contracts and effects before you depend on
it.
