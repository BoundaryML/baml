using Baml;
using Functions = BamlSdk.Functions;

var recursive = new BamlSdk.RecursiveNumbers(
    new List<BamlSdk.RecursiveNumbers>
    {
        new(1L),
        new(new List<BamlSdk.RecursiveNumbers> { new(2L), new(3L) }),
    });
var recursiveResult = Functions.RoundTripRecursiveNumbers(recursive);
var topLevel = recursiveResult.Value.AsT1;
if (topLevel.Count != 2
    || topLevel[0].Value.AsT0 != 1
    || !topLevel[1].Value.IsT1
    || !topLevel[1].Value.AsT1.Select(static item => item.Value.AsT0).SequenceEqual([2L, 3L]))
{
    throw new InvalidOperationException("The recursive type alias did not round-trip through BAML.");
}

var root = Path.Combine(Path.GetTempPath(), $"baml-csharp-glob-{Environment.ProcessId}");
Directory.CreateDirectory(root);
await File.WriteAllTextAsync(Path.Combine(root, "visible.txt"), "visible");
await File.WriteAllTextAsync(Path.Combine(root, ".hidden.txt"), "hidden");
Directory.CreateDirectory(Path.Combine(root, "directory.txt"));

try
{
    var original = Functions.CompileGlob("**/*.txt");
    using var glob = original.Clone();
    original.Dispose();

    if (!glob.Matches("nested/file.txt") || glob.Matches("nested/file.cs"))
    {
        throw new InvalidOperationException("The C# glob wrapper returned invalid match results.");
    }

    try
    {
        _ = original.Matches("visible.txt");
        throw new InvalidOperationException("A disposed glob remained usable.");
    }
    catch (ObjectDisposedException)
    {
    }

    var relative = await glob.ScanAsync(root);
    if (!relative.SequenceEqual(["visible.txt"], StringComparer.Ordinal))
    {
        throw new InvalidOperationException(
            $"The default glob scan returned an invalid result: {string.Join(", ", relative)}");
    }

    var options = new BamlGlobScanOptions(
        cwd: root,
        dot: true,
        absolute: true,
        followSymlinks: false,
        throwErrorOnBrokenSymlink: true,
        onlyFiles: false);
    var returnedOptions = Functions.EchoGlobScanOptions(options);
    if (returnedOptions.Cwd != root
        || returnedOptions.Dot != true
        || returnedOptions.Absolute != true
        || returnedOptions.FollowSymlinks != false
        || returnedOptions.ThrowErrorOnBrokenSymlink != true
        || returnedOptions.OnlyFiles != false)
    {
        throw new InvalidOperationException("Glob scan options did not round-trip through BAML.");
    }

    var absolute = glob.Scan(options).Order(StringComparer.Ordinal).ToArray();
    var expected = new[]
    {
        Path.Combine(root, ".hidden.txt"),
        Path.Combine(root, "directory.txt"),
        Path.Combine(root, "visible.txt"),
    }.Order(StringComparer.Ordinal).ToArray();
    if (!absolute.SequenceEqual(expected, StringComparer.Ordinal))
    {
        throw new InvalidOperationException(
            $"The configured glob scan returned {string.Join(", ", absolute)}; expected {string.Join(", ", expected)}.");
    }
}
finally
{
    Directory.Delete(root, recursive: true);
}

Console.WriteLine("C# glob integration passed.");
