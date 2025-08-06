import path from 'path';
import fs from 'fs/promises';
import dynamic from 'next/dynamic';

const PromptPreview = dynamic(() => import('./clientwrapper'), {});

// Function to load BAML file content from the file system
async function loadBamlFile(exampleName: string): Promise<string> {
  try {
    // Sanitize the example name to prevent directory traversal attacks
    const sanitizedExampleName = exampleName.replace(/[^a-zA-Z0-9-_]/g, '');
    if (sanitizedExampleName !== exampleName) {
      console.warn(
        `Example name was sanitized from ${exampleName} to ${sanitizedExampleName}`,
      );
    }

    const filePath = path.join(
      process.cwd(),
      'public',
      '_docs',
      sanitizedExampleName,
      'baml_src',
      'main.baml',
    );

    // Check if the file exists
    try {
      await fs.access(filePath);
    } catch (error) {
      console.warn(
        `BAML file not found for example ${sanitizedExampleName}, falling back to default example`,
      );
      return loadBamlFile('default-example');
    }

    return await fs.readFile(filePath, 'utf-8');
  } catch (error) {
    console.error(`Error loading BAML file for example ${exampleName}:`, error);
    // Return default BAML content if all else fails
    return `
      function Hi() -> string {
        client "openai/gpt-4o"
        prompt #"
          hi there
        "#
      }

      test HiTest {
        functions [Hi]
        args {

        }
      }
    `;
  }
}

export default async function EmbedComponent({
  searchParams,
}: {
  searchParams: Promise<{ id: string }>;
}) {
  const params = await searchParams;
  // Get example name from URL parameters, default to 'default-example' if not provided
  const exampleName =
    typeof params.id === 'string' ? params.id : 'default-example';
  console.log('exampleName', exampleName);

  // Load the BAML file content
  const bamlContent = await loadBamlFile(exampleName);

  return (
    <div className="flex justify-center items-center h-screen rounded-lg border-2 border-purple-900/30 overflow-y-clip">
      <div className="flex w-full h-full">
        <PromptPreview bamlContent={bamlContent} />
      </div>
    </div>
  );
}
