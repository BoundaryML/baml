// Test file system operations typecheck correctly

function ReadFile() -> string {
    let f = baml.fs.open("example.txt");
    f.read()
}

function ReadFileInline() -> string {
    baml.fs.open("data.json").read()
}

function ReadMultipleFiles() -> string {
    let f1 = baml.fs.open("file1.txt");
    let f2 = baml.fs.open("file2.txt");
    let content1 = f1.read();
    let content2 = f2.read();
    content1 + content2
}

function ReadAndClose() -> string {
    let f = baml.fs.open("example.txt");
    let content = f.read();
    f.close();
    content
}
