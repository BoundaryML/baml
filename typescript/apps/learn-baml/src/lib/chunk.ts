export interface DocChunk {
  id: string;
  title: string;
  url: string;
  content: string;
  section?: string;
}

/**
 * Chunk markdown content by headers, preserving context
 */
export function chunkMarkdown(
  content: string,
  metadata: { title: string; url: string },
  maxChunkSize = 1500
): DocChunk[] {
  const chunks: DocChunk[] = [];

  // Split by H2 headers
  const sections = content.split(/^##\s+/gm);

  for (let i = 0; i < sections.length; i++) {
    const section = sections[i].trim();
    if (!section) continue;

    // Extract section title (first line after split)
    const lines = section.split('\n');
    const sectionTitle = i === 0 ? metadata.title : lines[0].trim();
    const sectionContent = i === 0 ? section : lines.slice(1).join('\n').trim();

    // If section is small enough, add as single chunk
    if (sectionContent.length <= maxChunkSize) {
      chunks.push({
        id: `${metadata.url}#${slugify(sectionTitle)}`,
        title: metadata.title,
        url: metadata.url,
        content: sectionContent,
        section: sectionTitle,
      });
      continue;
    }

    // Split large sections by paragraphs
    const paragraphs = sectionContent.split(/\n\n+/);
    let currentChunk = '';
    let chunkIndex = 0;

    for (const paragraph of paragraphs) {
      if ((currentChunk + paragraph).length > maxChunkSize && currentChunk) {
        chunks.push({
          id: `${metadata.url}#${slugify(sectionTitle)}-${chunkIndex}`,
          title: metadata.title,
          url: metadata.url,
          content: currentChunk.trim(),
          section: sectionTitle,
        });
        currentChunk = paragraph;
        chunkIndex++;
      } else {
        currentChunk = currentChunk ? `${currentChunk}\n\n${paragraph}` : paragraph;
      }
    }

    if (currentChunk.trim()) {
      chunks.push({
        id: `${metadata.url}#${slugify(sectionTitle)}-${chunkIndex}`,
        title: metadata.title,
        url: metadata.url,
        content: currentChunk.trim(),
        section: sectionTitle,
      });
    }
  }

  return chunks;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
}
