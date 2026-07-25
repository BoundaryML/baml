//! MiniJinja-Contrib is a utility crate for [MiniJinja](https://github.com/mitsuhiko/minijinja)
//! that adds support for certain utilities that are too specific for the MiniJinja core.  This is
//! usually because they provide functionality that Jinja2 itself does not have.
//!
//! To add all of these to an environment you can use the [`add_to_environment`] function.
//!
//! ```
//! use minijinja::Environment;
//!
//! let mut env = Environment::new();
//! minijinja_contrib::add_to_environment(&mut env);
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

use minijinja::Environment;

/// Implements Python methods for better compatibility.
#[cfg(feature = "pycompat")]
pub mod pycompat;

/// Utility filters.
pub mod filters;

/// Globals
pub mod globals;

#[cfg(feature = "html_entities")]
mod html_entities;

#[cfg(feature = "rand")]
mod rand;

/// Registers all features of this crate with an [`Environment`].
///
/// All the filters that are available will be added, same with global
/// functions that exist.
///
/// **Note:** the `pycompat` support is intentionally not registered
/// with the environment.
pub fn add_to_environment(env: &mut Environment) {
    env.add_filter("pluralize", filters::pluralize);
    env.add_filter("filesizeformat", filters::filesizeformat);
    env.add_filter("truncate", filters::truncate);
    env.add_filter("striptags", filters::striptags);
    #[cfg(feature = "wordcount")]
    {
        env.add_filter("wordcount", filters::wordcount);
    }
    #[cfg(feature = "wordwrap")]
    {
        env.add_filter("wordwrap", filters::wordwrap);
    }
    #[cfg(feature = "datetime")]
    {
        env.add_filter("datetimeformat", filters::datetimeformat);
        env.add_filter("timeformat", filters::timeformat);
        env.add_filter("dateformat", filters::dateformat);
        env.add_function("now", globals::now);
    }
    #[cfg(feature = "rand")]
    {
        env.add_filter("random", filters::random);
        env.add_function("lipsum", globals::lipsum);
        env.add_function("randrange", globals::randrange);
    }
    env.add_function("cycler", globals::cycler);
    env.add_function("joiner", globals::joiner);
}
