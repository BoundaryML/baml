# Source filtering for crane builds
# Filters out build artifacts, VCS dirs, and other non-source files.
{ pkgs }:

pkgs.lib.cleanSourceWith {
  src = ../../engine;
  filter =
    path: type:
    let
      baseName = baseNameOf path;
    in
    !pkgs.lib.hasInfix "target" path
    && !pkgs.lib.hasInfix ".git" path
    && !pkgs.lib.hasInfix ".jj" path
    && !pkgs.lib.hasInfix ".so" path
    && !pkgs.lib.hasInfix ".node" path
    && !pkgs.lib.hasInfix "node_modules" path
    && baseName != "result";
}
