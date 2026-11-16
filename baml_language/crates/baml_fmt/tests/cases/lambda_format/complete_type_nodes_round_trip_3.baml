interface Factory { function make() -> (() -> int throws never) throws never; }
