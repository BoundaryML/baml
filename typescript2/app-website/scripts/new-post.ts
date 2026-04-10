#!/usr/bin/env tsx

import { existsSync } from 'node:fs';
import fs, { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  log as clackLog,
  confirm,
  group,
  intro,
  isCancel,
  note,
  outro,
  select,
  tasks,
  text,
} from '@clack/prompts';
import dedent from 'dedent-js';
import { glob } from 'glob';
import matter from 'gray-matter';
import { isPlainObject, isUndefined, kebabCase, omitBy } from 'lodash-es';
import pc from 'picocolors';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { $ } from 'zx';
import type { Author, OpenGraph, Post } from '../lib/types';

/**
 * Take the tagged string and remove indentation and word-wrapping.
 *
 * @package util
 */

function unwrap(
  strings: TemplateStringsArray | string,
  ...expressions: any[]
): string {
  // Handle plain strings
  if (typeof strings === 'string') {
    return dedent(strings);
  }

  // Handle template literals
  if (!strings || !Array.isArray(strings.raw)) {
    return '';
  }

  // Interleave the strings with the substituted expressions
  const result = strings.raw.reduce((acc, str, i) => {
    const expression = expressions[i] ?? '';
    return acc + str + expression;
  }, '');

  return dedent(result);
}

// Types
interface Args {
  verbose: boolean;
}

// CLI Configuration
const argv = yargs(hideBin(process.argv))
  .option('verbose', {
    alias: 'v',
    default: false,
    description: 'Run with verbose logging',
    type: 'boolean',
  })
  .help()
  .parse() as Args;

// Logging utility
const log = {
  error: (message: string) => {
    clackLog.error(message);
  },
  info: (message: string) => {
    if (argv.verbose) {
      clackLog.info(message);
    }
  },
  success: (message: string) => {
    clackLog.success(message);
  },
  warn: (message: string) => {
    if (argv.verbose) {
      clackLog.warn(message);
    }
  },
};

// Make zx silent by default, but enable if verbose
$.verbose = argv.verbose;

// Helper Functions
function handleCancel() {
  outro(pc.red('Operation cancelled'));
  process.exit(0);
}

function slugify(text?: string): string {
  if (!text) return '';
  return kebabCase(text.toLowerCase()).replace(/^-|-$/g, '');
}

function formatDate(date: Date): string {
  return date.toLocaleDateString('en-US', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  });
}

async function getGitUser(): Promise<string> {
  log.info('Checking git configuration...');
  const gitUser = (await $`git config user.name`.quiet()).stdout.trim();
  if (!gitUser) {
    outro(
      pc.red(
        'Error: Git user name not configured. Please run: git config --global user.name "Your Name"',
      ),
    );
    process.exit(1);
  }
  log.info(`Found git user: ${gitUser}`);
  return gitUser;
}

async function findExistingAuthors(): Promise<
  Map<string, Partial<Author> & { name: string }>
> {
  log.info('Scanning for existing authors...');
  const posts = await glob('posts/*.mdx');
  log.info(`Found ${posts.length} MDX files to scan`);
  const authors = new Map<string, Partial<Author> & { name: string }>();

  for (const post of posts) {
    log.info(`Checking post: ${post}`);
    try {
      const content = await fs.readFile(post, 'utf8');
      const { data } = matter(content);

      if (data.author?.name && data.author?.imageUrl) {
        const { name, imageUrl, linkedin } = data.author;
        log.info(`Found author: ${name} with image: ${imageUrl}`);
        authors.set(name, {
          imageUrl,
          linkedin,
          name,
        });
      } else {
        log.warn(`Found incomplete author data in ${post}`);
      }
    } catch (error) {
      log.warn(`Error reading ${post}: ${error}`);
    }
  }
  log.info(`Found ${authors.size} authors`);
  return authors;
}

async function getLinkedInProfileImage(
  linkedinUrl: string,
): Promise<string | undefined> {
  log.info(`Attempting to get LinkedIn profile image from: ${linkedinUrl}`);
  const username = linkedinUrl.split('/in/')[1]?.replace(/\/$/, '');
  if (!username) {
    log.warn(`Could not extract username from LinkedIn URL: ${linkedinUrl}`);
    return undefined;
  }
  log.info(`Extracted username: ${username}`);

  const localImagePath = `/profile-${username}.jpeg`;
  let imageUrl: string | undefined;

  // First task: Attempt to fetch and extract profile image URL
  await tasks([
    {
      task: async () => {
        try {
          log.info('Fetching LinkedIn profile page...');
          await $`curl -s -L ${linkedinUrl} -o /tmp/linkedin-profile.html`.quiet();
          const content = await $`cat /tmp/linkedin-profile.html`.quiet();
          const imageMatch = content.stdout.match(
            /profile-displayphoto-shrink_\d+_\d+\/\d+\/\d+\/\d+\/[^"]+/,
          );
          if (!imageMatch) {
            log.warn('Could not find profile image URL in LinkedIn page');
            return;
          }
          imageUrl = `https://media.licdn.com/dms/${imageMatch[0]}`;
          log.info(`Found LinkedIn image URL: ${imageUrl}`);
        } catch (e) {
          log.warn(`Error fetching LinkedIn profile: ${e}`);
          return;
        } finally {
          await $`rm -f /tmp/linkedin-profile.html`.quiet();
        }
      },
      title: 'Attempting to fetch LinkedIn profile image',
    },
  ]);

  // If we couldn't get the image URL, return early
  if (!imageUrl) {
    return undefined;
  }

  // Second task: Download the image if we have the URL
  await tasks([
    {
      task: async () => {
        try {
          log.info(`Downloading image to: public${localImagePath}`);
          await $`curl -s ${imageUrl} -o public${localImagePath}`.quiet();
          log.info('Successfully downloaded profile image');
        } catch (e) {
          log.warn(`Error downloading profile image: ${e}`);
          throw e;
        }
      },
      title: 'Downloading profile image',
    },
  ]).catch(() => {
    return undefined;
  });

  // Clean up any remaining temp files
  await $`rm -f /tmp/linkedin-profile.html`.quiet();

  return localImagePath;
}

async function promptForNewAuthor(
  gitUser: string,
): Promise<Partial<Author> & { name: string }> {
  log.info(`Prompting for new author details for: ${gitUser}`);
  const linkedinInput = await text({
    initialValue: `https://linkedin.com/in/${kebabCase(gitUser)}`,
    message:
      'Enter your LinkedIn URL (optional) Find it here: https://www.linkedin.com',
    placeholder: 'https://linkedin.com/in/username',
    validate: (value) => {
      // Empty is valid since it's optional
      if (!value.trim()) return;

      try {
        // Check if it's a valid URL
        const url = new URL(value.trim());
        // Check if it's a LinkedIn profile URL
        if (
          !url.hostname.includes('linkedin.com') ||
          !url.pathname.startsWith('/in/')
        ) {
          return 'Please enter a valid LinkedIn profile URL (e.g., https://linkedin.com/in/username) or leave empty';
        }
      } catch (e) {
        return 'Please enter a valid URL (must start with http:// or https://) or leave empty';
      }
    },
  });

  if (isCancel(linkedinInput)) handleCancel();

  // Handle LinkedIn URL and image
  let imageUrl: string | undefined;
  let linkedinUrl: string | undefined;

  if (linkedinInput) {
    const input = String(linkedinInput).trim();
    if (input) {
      const fetchedImageUrl = await getLinkedInProfileImage(input);
      if (fetchedImageUrl) {
        imageUrl = fetchedImageUrl;
      }
    }
  }

  // If no image URL yet, prompt for manual image URL
  if (!imageUrl) {
    const manualImageInfo = await text({
      message: 'Enter your profile image URL (optional):',
      placeholder: '/profile-image.jpeg',
    });
    if (isCancel(manualImageInfo)) handleCancel();
    imageUrl = String(manualImageInfo);
  }

  return {
    imageUrl,
    linkedin: linkedinUrl,
    name: gitUser,
  };
}

async function selectAuthor(
  authors: Map<string, Partial<Author> & { name: string }>,
  gitUser: string,
): Promise<Partial<Author> & { name: string }> {
  const authorsList = Array.from(authors.keys());
  // Check if git user matches any existing author (case insensitive)
  const matchingAuthor = authorsList.find(
    (name) => name.toLowerCase() === gitUser.toLowerCase(),
  );

  const authorChoice = await select({
    initialValue: matchingAuthor, // Pre-select the matching author if found
    message: 'Select the author',
    options: [
      // Only show "Add new author" if git user doesn't match any existing author
      ...(!matchingAuthor
        ? [
            {
              label: `+ Add new author (${gitUser}) - found from git config`,
              value: 'new',
            },
          ]
        : []),
      ...authorsList.map((a) => ({ label: a, value: a })),
    ],
  });

  if (isCancel(authorChoice)) handleCancel();

  // Ensure authorChoice is a string before using it
  const choice = String(authorChoice);
  return choice === 'new'
    ? await promptForNewAuthor(gitUser)
    : authors.get(choice)!;
}

async function promptForOg(
  postTitle: string,
  postDescription: string,
): Promise<OpenGraph | undefined> {
  note(
    pc.white(
      unwrap`OG (Open Graph) Configuration

      If you don't provide custom OG data, the post's title and description will be used.
      You can customize the OG title, description, and image to optimize social media sharing.

      Example image path:
      /blog/my-post/og-image.png`,
    ),
  );
  const useCustomOg = await confirm({
    initialValue: false,
    message: 'Would you like to specify custom OG data?',
  });

  if (!isCancel(useCustomOg) && useCustomOg) {
    const ogTitleInput = await text({
      initialValue: postTitle,
      message: 'Enter the OG title (optional, press Enter to use post title):',
      placeholder: postTitle,
    });
    if (isCancel(ogTitleInput)) handleCancel();

    const ogDescriptionInput = await text({
      initialValue: postDescription,
      message:
        'Enter the OG description (optional, press Enter to use post description):',
      placeholder: postDescription,
    });
    if (isCancel(ogDescriptionInput)) handleCancel();

    const ogImageInput = await text({
      initialValue: `/blog/${slugify(postTitle)}/og-image.png`,
      message:
        'Enter the OG image path (relative to public directory) (optional):',
      placeholder: '/blog/my-post/og-image.png',
    });
    if (isCancel(ogImageInput)) handleCancel();

    const ogTitle = String(ogTitleInput).trim();
    const ogDescription = String(ogDescriptionInput).trim();
    const ogImage = String(ogImageInput).trim();

    return {
      ...(ogTitle && { title: ogTitle }),
      ...(ogDescription && { description: ogDescription }),
      ...(ogImage && { image: ogImage }),
    };
  }

  return undefined;
}

async function createDirectories(
  date: string,
  slug: string,
): Promise<{ postsDir: string; mediaDir?: string }> {
  log.info(`Planning directories for post dated ${date} with slug ${slug}`);
  const postsDir = path.join(process.cwd(), 'posts');

  const createMediaDir = await confirm({
    initialValue: false,
    message: 'Would you like to create a media directory for this post?',
  });

  if (isCancel(createMediaDir)) handleCancel();

  if (createMediaDir) {
    const mediaPath = `/blog/${date}-${slug}`;
    const fullMediaDir = path.join(process.cwd(), 'public', mediaPath);
    log.info(`Will create media directory at: ${fullMediaDir}`);
    return { mediaDir: mediaPath, postsDir };
  }

  return { postsDir };
}

async function createGitBranch(
  gitUser: string,
  slug: string,
  title: string,
  mdxPath: string,
  mediaDir?: string,
): Promise<string> {
  log.info(`Planning git branch for user: ${gitUser}, slug: ${slug}`);
  const safeGitUser = kebabCase(gitUser);
  const defaultBranchName = `${safeGitUser}/blog/${slug}`;
  log.info(`Generated default branch name: ${defaultBranchName}`);

  const branchNameInput = await text({
    initialValue: defaultBranchName,
    message: 'Enter branch name:',
    placeholder: defaultBranchName,
    validate: (value) => {
      if (!value.trim()) return 'Branch name cannot be empty';
      // Ensure the branch name doesn't start with a slash
      if (value.startsWith('/')) return 'Branch name cannot start with a slash';
      return;
    },
  });

  if (isCancel(branchNameInput)) handleCancel();
  return String(branchNameInput);
}

function cleanUndefined<T extends Record<string, any>>(obj: T): T {
  return omitBy(
    Object.entries(obj).reduce(
      (acc, [key, value]) => ({
        ...acc,
        [key]: isPlainObject(value) ? cleanUndefined(value) : value,
      }),
      {} as T,
    ),
    isUndefined,
  ) as T;
}

function generateMdxContent(post: Partial<Post>): string {
  log.info(`Generating MDX content for post: ${post.title}`);

  // Create frontmatter object and clean undefined values recursively
  const frontmatter = cleanUndefined({
    author: post.author,
    date: post.date,
    description: post.description,
    slug: post.slug,
    tags: post.tags,
    title: post.title,
    ...(post.og && { og: post.og }),
    isPublished: true,
  });

  // Use gray-matter to stringify with proper YAML formatting
  return matter.stringify(post.body || '', frontmatter);
}

async function generatePost() {
  let currentStep = 'initialization';
  try {
    intro(pc.blue('📝 Blog Post Generator'));
    log.info('Starting blog post generation process');

    note(
      pc.white(
        unwrap`This wizard will help you create a new blog post for the Boundary website.

✨ We'll automatically:
1. Generate an MDX file in /posts with frontmatter
2. Search for existing author info in other posts
3. Use git config for author details if available
4. Create a media directory for assets (optional)
5. Set up a git branch (optional)

💡 Use --verbose for detailed logs

Press Ctrl+C to cancel.`,
      ),
    );

    log.info('Starting post generation...');

    // Collect all information first
    currentStep = 'getting git user';
    const gitUser = await getGitUser();

    currentStep = 'gathering post information';
    const postInfo = await group(
      {
        description: () =>
          text({
            message: 'Enter a brief description:',
            placeholder: 'A short description of your blog post',
            validate: (value) =>
              !value ? 'Please provide a description' : undefined,
          }),
        tags: () =>
          text({
            message: 'Enter tags (comma-separated) (optional):',
            placeholder: 'ai, llm, typescript',
          }),
        title: () =>
          text({
            message: 'What is the title of your blog post?',
            placeholder: 'My Awesome Blog Post',
            validate: (value) =>
              !value ? 'Please provide a title' : undefined,
          }),
        type: () =>
          select({
            initialValue: 'blog',
            message: 'What type of post is this?',
            options: [
              { label: 'Blog Post', value: 'blog' },
              { label: 'Research Paper', value: 'research' },
            ],
          }),
      },
      {
        onCancel: () => {
          handleCancel();
        },
      },
    );

    // Ensure strings
    const postTitle = String(postInfo.title);
    const postDescription = String(postInfo.description);
    const postType = String(postInfo.type);
    const tags = [
      postType,
      ...(postInfo.tags
        ? String(postInfo.tags)
            .split(',')
            .map((t) => t.trim())
        : []),
    ];

    log.info(`Title set: ${postTitle}`);

    // Handle author selection
    currentStep = 'selecting author';
    const authors = await findExistingAuthors();
    const author = await selectAuthor(authors, gitUser);

    // Handle OG data
    currentStep = 'gathering OG data';
    const ogData = await promptForOg(postTitle, postDescription);

    // Generate slug and dates
    currentStep = 'generating slug and dates';
    const slug = slugify(postTitle);
    const isoDate = new Date().toISOString().split('T')[0] as string;
    const formattedDate = formatDate(new Date());

    // Create directories
    currentStep = 'planning directories';
    const { postsDir, mediaDir } = await createDirectories(isoDate, slug);

    // Construct the media path for the blog post
    const mediaPath = mediaDir ? `/blog/${isoDate}-${slug}` : '/';

    const body = unwrap`
  ## Introduction

  Welcome to this example blog post! Here we'll demonstrate various markdown features and media handling capabilities.

  ### Text Formatting

  You can write text in **bold**, *italic*, or ***both***. You can also use ~~strikethrough~~ for removed content.

  ### Lists

  Here's an unordered list:
  - First item
  - Second item
    - Nested item
    - Another nested item
  - Third item

  And a numbered list:
  1. First step
  2. Second step
  3. Third step

  ### Code Blocks

  Inline code looks like this: \`const example = "hello world"\`

  ### Blockquotes

  > This is a blockquote. You can use it to highlight important information or quotes.
  >
  > It can span multiple lines and even contain other markdown elements like **bold text**.

  ### Media Examples

  Here's how to include an image from your media directory:

  ![Example diagram](${mediaPath}/example-diagram.png)

  You can also embed videos using our custom MDX components:

  <Video src="${mediaPath}/demo-video.mp4" />

  ### Tables

  | Feature | Description | Support |
  |---------|-------------|---------|
  | Tables | Organize data | ✅ |
  | Images | Show visuals | ✅ |
  | Code | Share snippets | ✅ |

  ### Links

  You can create [inline links](https://boundaryml.com) or reference our [documentation](/docs).


  ### Mathematical Equations

  You can include inline math like this: $E = mc^2$

  ---

  That's it! This example shows most of the markdown features you can use in your blog posts. Remember to replace the example media paths with your actual media files.
  `;

    // Create post object
    currentStep = 'creating post object';
    const post: Partial<Post> = {
      author,
      body,
      date: formattedDate,
      description: postDescription,
      isPublished: true,
      slug,
      tags,
      title: postTitle,
      ...(ogData && { og: ogData }),
    };

    currentStep = 'preparing MDX file';
    const filename = `${isoDate}-${slug}.mdx`;
    const mdxPath = path.join(postsDir, filename);
    const mdxContent = generateMdxContent(post);

    currentStep = 'planning git branch';
    const shouldCreateBranch = await confirm({
      initialValue: true,
      message: 'Would you like to create a new git branch for this post?',
    });

    if (isCancel(shouldCreateBranch)) handleCancel();
    const branchName = shouldCreateBranch
      ? await createGitBranch(gitUser, slug, postTitle, mdxPath, mediaDir)
      : undefined;

    // Now create everything in sequence
    currentStep = 'creating files and directories';
    await tasks([
      {
        task: async () => {
          if (!existsSync(postsDir)) {
            log.info(`Creating posts directory: ${postsDir}`);
            await mkdir(postsDir, { recursive: true });
          }
        },
        title: 'Creating posts directory',
      },
      ...(mediaDir
        ? [
            {
              task: async () => {
                const fullMediaDir = path.join(
                  process.cwd(),
                  'public',
                  mediaDir,
                );
                log.info(`Creating media directory: ${fullMediaDir}`);
                await mkdir(fullMediaDir, { recursive: true });
              },
              title: 'Creating media directory',
            },
          ]
        : []),
      {
        task: async () => {
          log.info(`Writing MDX file: ${mdxPath}`);
          await writeFile(mdxPath, mdxContent);
        },
        title: 'Writing MDX file',
      },
      ...(branchName
        ? [
            {
              task: async () => {
                log.info(`Creating git branch: ${branchName}`);
                await $`git checkout -b ${branchName}`.quiet();
                await $`git add ${mdxPath}`.quiet();
                if (mediaDir) {
                  await $`git add public${mediaDir}`.quiet();
                }
                await $`git commit -m "feat(blog): add ${postTitle}"`.quiet();
              },
              title: 'Setting up git branch',
            },
          ]
        : []),
    ]);

    // Success message
    outro(
      pc.green(`
✅ Successfully created:
   - Post: ${pc.blue(mdxPath)}${mediaDir ? `\n   - Media directory: ${pc.blue(mediaDir)}` : ''}${
     branchName ? `\n   - Branch: ${pc.blue(branchName)}` : ''
   }
    `),
    );
  } catch (err) {
    throw new Error(
      `Failed during ${currentStep}: ${err instanceof Error ? err.message : String(err)}`,
    );
  }
}

generatePost().catch((err: unknown) => {
  const errorMessage = err instanceof Error ? err.message : String(err);
  console.error(
    'Error occurred. Re-run with -v flag for verbose output to debug the issue.',
  );
  console.error('Example: ./scripts/new-post.ts -v');
  console.error('Failed step:', errorMessage.split(':')[0]);
  console.error(err);
  process.exit(1);
});
