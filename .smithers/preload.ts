// Registers the MDX bun loader so `.smithers/prompts/*.mdx` prompt components can
// be imported by the workflow and its tests. Loaded via ./bunfig.toml preload.
import { mdxPlugin } from "smthrs";

mdxPlugin();
