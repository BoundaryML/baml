import Anser from 'anser'
import type React from 'react'

const getLinks = (text: string) => {
  const txt = text.replace(/[^<>\s]+\.log\b/gm, (str) => `<a href="vscode://file/${str}">${str}</a>`)
  const urlRegex = /(<span class="ansi-(?:[^"]+)">)(https?:\/\/[^\s<]+)(<\/span>)/g
  // Replace log files with links (lines with only *.log)
  return txt.replace(urlRegex, (match, startTag, url, endTag) => {
    // Replace the span with an anchor tag
    return `${startTag}<a href="${url}" target="_blank" rel="noopener noreferrer">${url}</a>${endTag}`
  })
}

const AnsiText: React.FC<{ text: string; className: string }> = ({ text, className }) => {
  // use tailwind vscode classes in App.css with use_classes
  const html = Anser.ansiToHtml(Anser.escapeForHtml(text), { use_classes: true })

  return <pre className={className} dangerouslySetInnerHTML={{ __html: getLinks(html) }} />
}

export default AnsiText
