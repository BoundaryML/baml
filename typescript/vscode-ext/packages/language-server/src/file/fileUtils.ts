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


export function gatherFiles(dir: string, debug: boolean = false, fileList: string[] = []): string[] {
  let uri = URI.parse(dir);
  let dirPath = uri.fsPath;
  // if (debug) {
  console.log(`Gathering files from ${dirPath}. uri: ${uri.toString()} ${JSON.stringify(uri.toJSON())}`);
  // }

  // this line is failing
  const files = fs.readdirSync(dirPath);
  // if (debug) {
  console.log(`\tFound ${files.length} files`);
  // }

  files.forEach((file) => {
    const filePath = path.join(dirPath, file);
    const fileStat = fs.statSync(filePath);
    if (debug) {
      console.log(`\tChecking ${filePath}`);
      console.log(`\tisDirectory: ${fileStat.isDirectory()}`);
    }

    if (fileStat.isDirectory()) {
      // TODO: use the VSCode URI class to create the URI
      gatherFiles(`file:///${filePath}`, debug, fileList);
    } else {
      // check if it has .baml extension
      if (filePath.endsWith('.baml') || filePath.endsWith('.json')) {
        fileList.push(filePath);
      }
    }
  });

  return fileList.map((filePath) => URI.file(filePath).toString());
}


export function convertToTextDocument(filePath: string): TextDocument {
  const fileContent = fs.readFileSync(URI.parse(filePath).fsPath, 'utf-8');
  const fileExtension = path.extname(filePath);
  return TextDocument.create(
    filePath.toString(),
    fileExtension === '.baml' ? 'baml' : 'json',
    1,
    fileContent
  );
}
