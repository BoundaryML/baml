using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Xml;
using System.Xml.Linq;

const string relationshipsPath = "_rels/.rels";
const string corePropertiesPrefix = "package/services/metadata/core-properties/";
const string corePropertiesRelationship =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";

if (args.Length != 2)
{
    Console.Error.WriteLine("usage: Baml.NuGet.Normalize <input.nupkg> <output.nupkg>");
    return 2;
}

var inputPath = Path.GetFullPath(args[0]);
var outputPath = Path.GetFullPath(args[1]);
if (string.Equals(inputPath, outputPath, StringComparison.Ordinal))
{
    Console.Error.WriteLine("input and output package paths must differ");
    return 2;
}

try
{
    Normalize(inputPath, outputPath);
    return 0;
}
catch (Exception error)
{
    Console.Error.WriteLine(error.Message);
    return 1;
}

static void Normalize(string inputPath, string outputPath)
{
    using var inputStream = File.OpenRead(inputPath);
    using var input = new ZipArchive(inputStream, ZipArchiveMode.Read, leaveOpen: false);
    if (input.GetEntry(".signature.p7s") is not null)
    {
        throw new InvalidDataException("signed NuGet packages cannot be normalized; normalize before signing");
    }

    foreach (var entry in input.Entries)
    {
        ValidateEntryName(entry.FullName);
    }

    var coreEntries = input.Entries
        .Where(static entry =>
            entry.FullName.StartsWith(corePropertiesPrefix, StringComparison.Ordinal)
            && entry.FullName.EndsWith(".psmdcp", StringComparison.Ordinal))
        .ToArray();
    if (coreEntries.Length != 1)
    {
        throw new InvalidDataException($"expected exactly one NuGet core-properties part, found {coreEntries.Length}");
    }

    var coreHash = Convert.ToHexString(SHA256.HashData(ReadAllBytes(coreEntries[0]))).ToLowerInvariant();
    var canonicalCorePath = $"{corePropertiesPrefix}{coreHash[..32]}.psmdcp";
    var canonicalRelationshipId = $"R{coreHash[..16].ToUpperInvariant()}";
    var destinations = input.Entries
        .Select(entry => new PackageEntry(
            entry,
            ReferenceEquals(entry, coreEntries[0]) ? canonicalCorePath : entry.FullName))
        .OrderBy(static item => item.Destination, StringComparer.Ordinal)
        .ToArray();
    var duplicate = destinations.GroupBy(static item => item.Destination, StringComparer.Ordinal)
        .FirstOrDefault(static group => group.Count() > 1);
    if (duplicate is not null)
    {
        throw new InvalidDataException($"normalization produced duplicate package entry {duplicate.Key}");
    }

    Directory.CreateDirectory(Path.GetDirectoryName(outputPath)!);
    try
    {
        using var outputStream = new FileStream(outputPath, FileMode.Create, FileAccess.ReadWrite, FileShare.None);
        using var output = new ZipArchive(outputStream, ZipArchiveMode.Create, leaveOpen: false);
        foreach (var item in destinations)
        {
            var outputEntry = output.CreateEntry(item.Destination, CompressionLevel.Optimal);
            outputEntry.LastWriteTime = new DateTimeOffset(1980, 1, 1, 0, 0, 0, TimeSpan.Zero);
            outputEntry.ExternalAttributes = item.Source.ExternalAttributes;
            using var destination = outputEntry.Open();
            if (string.Equals(item.Source.FullName, relationshipsPath, StringComparison.Ordinal))
            {
                WriteRelationships(
                    item.Source,
                    destination,
                    canonicalCorePath,
                    canonicalRelationshipId);
            }
            else
            {
                using var source = item.Source.Open();
                source.CopyTo(destination);
            }
        }
    }
    catch
    {
        File.Delete(outputPath);
        throw;
    }
}

static void WriteRelationships(
    ZipArchiveEntry source,
    Stream destination,
    string canonicalCorePath,
    string canonicalRelationshipId)
{
    using var sourceStream = source.Open();
    var document = XDocument.Load(sourceStream, LoadOptions.None);
    XNamespace relationships = "http://schemas.openxmlformats.org/package/2006/relationships";
    var coreRelationship = document.Root?.Elements(relationships + "Relationship")
        .SingleOrDefault(element =>
            string.Equals(
                (string?)element.Attribute("Type"),
                corePropertiesRelationship,
                StringComparison.Ordinal))
        ?? throw new InvalidDataException("NuGet package has no core-properties relationship");
    coreRelationship.SetAttributeValue("Target", $"/{canonicalCorePath}");
    coreRelationship.SetAttributeValue("Id", canonicalRelationshipId);

    using var writer = XmlWriter.Create(destination, new XmlWriterSettings
    {
        Encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
        Indent = true,
        NewLineChars = "\n",
        NewLineHandling = NewLineHandling.Replace,
        OmitXmlDeclaration = false,
        CloseOutput = false,
    });
    document.Save(writer);
}

static byte[] ReadAllBytes(ZipArchiveEntry entry)
{
    using var source = entry.Open();
    using var buffer = new MemoryStream(checked((int)entry.Length));
    source.CopyTo(buffer);
    return buffer.ToArray();
}

static void ValidateEntryName(string name)
{
    if (string.IsNullOrEmpty(name)
        || name[0] == '/'
        || name.Contains('\\')
        || name.Split('/').Any(static segment => segment is "." or ".."))
    {
        throw new InvalidDataException($"unsafe NuGet package entry name: {name}");
    }
}

internal sealed record PackageEntry(ZipArchiveEntry Source, string Destination);
