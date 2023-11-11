import * as path from 'path';
import * as fs from 'fs';
import { TextDocument } from 'vscode-languageserver-textdocument';
import { URI } from 'vscode-uri';

// export function findTopLevelParent(filePath: string) {
//   let currentPath = filePath;
//   let parentDir: string | null = null;

//   while (currentPath !== path.parse(currentPath).root) {
//     currentPath = path.dirname(currentPath);
//     if (path.basename(currentPath) === 'baml_src') {
//       parentDir = currentPath;
//       break;
//     }
//   }


//   if (parentDir !== null) {
//     return parentDir;
//   }
//   return null;
// }


export function gatherFiles(dir: string, fileList: string[] = []): string[] {
  let uri = URI.parse(dir);
  let dirPath = uri.fsPath;
  const files = fs.readdirSync(dirPath);

  files.forEach((file) => {
    const filePath = path.join(dirPath, file);
    const fileStat = fs.statSync(filePath);

    if (fileStat.isDirectory()) {
      gatherFiles(filePath, fileList);
    } else {
      // check if it has .baml extension
      if (filePath.endsWith('.baml')) {
        fileList.push(filePath);
      }
    }
  });

  return fileList.map((filePath) => URI.file(filePath).toString());
}


export function convertToTextDocument(filePath: string): TextDocument {
  const fileContent = fs.readFileSync(URI.parse(filePath).fsPath, 'utf-8');
  return TextDocument.create(
    filePath.toString(),
    'baml',
    1,
    fileContent
  );
}
