import React from 'react'
import Anser from 'anser'

const getLinks = (text: string) => {
  let txt = text.replace(/(https?:\/\/[^\s]+)/gm, (str) => `<a href="${str}">${str}</a>`)
  // Replace log files with links (lines with only *.log)
  txt = txt.replace(/[^<>\s]+\.log\b/gm, (str) => `<a href="vscode://file/${str}">${str}</a>`)

  return txt
}

const AnsiText: React.FC<{ text: string; className: string }> = ({ text, className }) => {
  // TODO: Use tailwind here by calling Anser.ansiToJson(text) and then render the json.
  const html = Anser.ansiToHtml(text)

  return <pre className={className} dangerouslySetInnerHTML={{ __html: getLinks(html) }} />
}

export default AnsiText
