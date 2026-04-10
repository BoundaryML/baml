const fs = require('fs');
const path = require('path');

function directoryNameToTitle(dirName) {
  // Strip numeric prefix and first dash, then convert remaining dashes to spaces
  const withoutPrefix = dirName.replace(/^\d+-/, '');
  const withSpaces = withoutPrefix.replace(/-/g, ' ');

  // Convert to Title Case
  return withSpaces.replace(
    /\w\S*/g,
    (txt) => txt.charAt(0).toUpperCase() + txt.substr(1).toLowerCase(),
  );
}

function generateLessonsJson() {
  console.log('Generating lessons.json...');

  const lessonsDir = path.join(__dirname, '../lessons');
  const lessons = [];

  // Get all lesson directories
  const lessonDirs = fs
    .readdirSync(lessonsDir)
    .filter((item) => {
      const itemPath = path.join(lessonsDir, item);
      return fs.statSync(itemPath).isDirectory();
    })
    .sort(); // Sort to ensure consistent order

  lessonDirs.forEach((lessonId) => {
    console.log(`Processing lesson: ${lessonId}`);

    const lessonDir = path.join(lessonsDir, lessonId);
    const pagesDir = path.join(lessonDir, 'pages');

    const lesson = {
      id: lessonId,
      title: directoryNameToTitle(lessonId),
      pages: [],
    };

    // Add lesson overview if it exists
    const overviewPath = path.join(lessonDir, 'index.md');
    if (fs.existsSync(overviewPath)) {
      lesson.overview = fs.readFileSync(overviewPath, 'utf-8');
    }

    // Process pages
    if (fs.existsSync(pagesDir)) {
      const pageDirs = fs
        .readdirSync(pagesDir)
        .filter((item) => {
          const itemPath = path.join(pagesDir, item);
          return fs.statSync(itemPath).isDirectory();
        })
        .sort(); // Sort to ensure consistent order

      pageDirs.forEach((pageId) => {
        console.log(`  Processing page: ${pageId}`);

        const pageDir = path.join(pagesDir, pageId);
        const pageConfigPath = path.join(pageDir, 'page.json');

        if (!fs.existsSync(pageConfigPath)) {
          console.warn(`Skipping ${pageId}: no page.json`);
          return;
        }

        const pageConfig = JSON.parse(fs.readFileSync(pageConfigPath, 'utf-8'));
        const pageContent = fs.readFileSync(
          path.join(pageDir, 'index.md'),
          'utf-8',
        );

        const page = {
          ...pageConfig,
          id: pageId,
          title: directoryNameToTitle(pageId),
          content: pageContent,
          fileContents: {},
        };

        // Read all files in the page directory
        const pageFiles = fs.readdirSync(pageDir);
        pageFiles.forEach((fileName) => {
          const filePath = path.join(pageDir, fileName);
          const stat = fs.statSync(filePath);

          // Skip directories and config files
          if (
            stat.isDirectory() ||
            fileName === 'page.json' ||
            fileName === 'index.md'
          ) {
            return;
          }

          console.log(`    Loading file: ${fileName}`);
          page.fileContents[fileName] = fs.readFileSync(filePath, 'utf-8');
        });

        lesson.pages.push(page);
      });
    }

    lessons.push(lesson);
  });

  // Write the big JSON file
  const outputPath = path.join(__dirname, '../src/lib/lessons.json');
  fs.writeFileSync(outputPath, JSON.stringify(lessons, null, 2));
  console.log('✅ Generated lessons.json');
}

generateLessonsJson();
