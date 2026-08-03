using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Xml;
using System.Xml.Linq;

internal static class Program
{
    private const string ContentTypesEntryName = "[Content_Types].xml";
    private const string RootRelationshipsEntryName = "_rels/.rels";
    private const string SignatureEntryName = ".signature.p7s";
    private const string CanonicalCorePropertiesEntryName =
        "package/services/metadata/core-properties/core-properties.psmdcp";
    private const string CorePropertiesRelationshipType =
        "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
    private const int CanonicalFileExternalAttributes =
        unchecked((int)0x81A40000); // regular file, Unix mode 0644
    private static readonly DateTimeOffset CanonicalTimestamp =
        new(1980, 1, 1, 0, 0, 0, TimeSpan.Zero);

    public static int Main(string[] args)
    {
        if (args is ["--self-test"])
        {
            try
            {
                RunSelfTests();
                Console.WriteLine("nuget_payload_comparison_tests=ok");
                return 0;
            }
            catch (Exception exception)
            {
                Console.Error.WriteLine(
                    $"BAML NuGet self-test failed: {exception.Message}");
                return 1;
            }
        }

        if (args is ["compare", string expectedPath, string actualPath])
        {
            try
            {
                CompareProductPayload(expectedPath, actualPath);
                Console.WriteLine("nuget_product_payload=identical");
                return 0;
            }
            catch (Exception exception)
            {
                Console.Error.WriteLine(
                    $"BAML NuGet payload comparison failed: {exception.Message}");
                return 1;
            }
        }

        if (args.Length != 2)
        {
            Console.Error.WriteLine(
                "Usage: Baml.NuGetNormalizer <unsigned-input.nupkg> <normalized-output.nupkg>\n"
                + "       Baml.NuGetNormalizer compare <verified-unsigned.nupkg> <registry-signed.nupkg>\n"
                + "       Baml.NuGetNormalizer --self-test");
            return 2;
        }

        try
        {
            string inputPath = Path.GetFullPath(args[0]);
            string outputPath = Path.GetFullPath(args[1]);
            Normalize(inputPath, outputPath, allowRepositorySignature: false);

            using FileStream output = File.OpenRead(outputPath);
            string digest = Convert.ToHexStringLower(SHA256.HashData(output));
            Console.WriteLine($"normalized_package={outputPath}");
            Console.WriteLine($"sha256={digest}");
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(
                $"BAML NuGet normalization failed: {exception.Message}");
            return 1;
        }
    }

    private static void Normalize(
        string inputPath,
        string outputPath,
        bool allowRepositorySignature)
    {
        if (StringComparer.Ordinal.Equals(inputPath, outputPath))
        {
            throw new InvalidOperationException(
                "Input and output paths must be different.");
        }

        if (!File.Exists(inputPath))
        {
            throw new FileNotFoundException(
                "The input package does not exist.",
                inputPath);
        }

        if (File.Exists(outputPath))
        {
            throw new IOException(
                "The output path already exists; package normalization never overwrites artifacts.");
        }

        string? outputDirectory = Path.GetDirectoryName(outputPath);
        if (String.IsNullOrEmpty(outputDirectory)
            || !Directory.Exists(outputDirectory))
        {
            throw new DirectoryNotFoundException(
                $"The output directory does not exist: {outputDirectory}");
        }

        string temporaryPath = Path.Combine(
            outputDirectory,
            $".{Path.GetFileName(outputPath)}.{Guid.NewGuid():N}.tmp");

        try
        {
            using (FileStream inputStream = new(
                       inputPath,
                       FileMode.Open,
                       FileAccess.Read,
                       FileShare.Read,
                       bufferSize: 1 << 20,
                       FileOptions.SequentialScan))
            using (ZipArchive inputArchive = new(
                       inputStream,
                       ZipArchiveMode.Read,
                       leaveOpen: false,
                       entryNameEncoding: Encoding.UTF8))
            {
                IReadOnlyList<InputEntry> entries =
                    ReadAndValidateEntries(
                        inputArchive,
                        allowRepositorySignature);
                NormalizedMetadata metadata = NormalizeMetadata(entries);

                using FileStream temporaryStream = new(
                    temporaryPath,
                    FileMode.CreateNew,
                    FileAccess.ReadWrite,
                    FileShare.None,
                    bufferSize: 1 << 20,
                    FileOptions.SequentialScan);
                using (ZipArchive outputArchive = new(
                           temporaryStream,
                           ZipArchiveMode.Create,
                           leaveOpen: true,
                           entryNameEncoding: Encoding.UTF8))
                {
                    WriteNormalizedEntries(
                        entries,
                        metadata,
                        outputArchive);
                }

                temporaryStream.Flush(flushToDisk: true);
            }

            File.Move(temporaryPath, outputPath);
        }
        finally
        {
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    private static IReadOnlyList<InputEntry> ReadAndValidateEntries(
        ZipArchive archive,
        bool allowRepositorySignature)
    {
        List<InputEntry> entries = new(archive.Entries.Count);
        HashSet<string> names = new(StringComparer.OrdinalIgnoreCase);

        foreach (ZipArchiveEntry entry in archive.Entries)
        {
            ValidateEntryName(entry.FullName);
            if (!names.Add(entry.FullName))
            {
                throw new InvalidDataException(
                    $"The package contains a duplicate or case-colliding entry: {entry.FullName}");
            }

            if (StringComparer.OrdinalIgnoreCase.Equals(
                    entry.FullName,
                    SignatureEntryName))
            {
                if (allowRepositorySignature)
                {
                    continue;
                }

                throw new InvalidDataException(
                    "The package is already signed. Normalize unsigned bytes before signing.");
            }

            entries.Add(new InputEntry(entry.FullName, entry));
        }

        RequireSingleEntry(entries, ContentTypesEntryName);
        RequireSingleEntry(entries, RootRelationshipsEntryName);
        return entries;
    }

    private static void ValidateEntryName(string name)
    {
        if (String.IsNullOrWhiteSpace(name)
            || name.StartsWith("/", StringComparison.Ordinal)
            || name.Contains('\\', StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                $"The package contains an unsafe entry name: {name}");
        }

        string[] segments = name.Split('/');
        if (segments.Any(
                segment => segment.Length == 0
                           || StringComparer.Ordinal.Equals(segment, ".")
                           || StringComparer.Ordinal.Equals(segment, "..")))
        {
            throw new InvalidDataException(
                $"The package contains an unsafe entry name: {name}");
        }
    }

    private static void RequireSingleEntry(
        IReadOnlyList<InputEntry> entries,
        string requiredName)
    {
        int count = entries.Count(
            entry => StringComparer.Ordinal.Equals(
                entry.Name,
                requiredName));
        if (count != 1)
        {
            throw new InvalidDataException(
                $"The package must contain exactly one {requiredName} entry; found {count}.");
        }
    }

    private static NormalizedMetadata NormalizeMetadata(
        IReadOnlyList<InputEntry> entries)
    {
        InputEntry relationshipsEntry = entries.Single(
            entry => StringComparer.Ordinal.Equals(
                entry.Name,
                RootRelationshipsEntryName));
        XDocument relationshipsDocument = LoadXml(
            ReadAllBytes(relationshipsEntry.Entry),
            RootRelationshipsEntryName);

        XNamespace relationshipsNamespace =
            "http://schemas.openxmlformats.org/package/2006/relationships";
        XElement relationshipsRoot =
            relationshipsDocument.Root
            ?? throw new InvalidDataException(
                "The root relationships document has no root element.");
        if (relationshipsRoot.Name
            != relationshipsNamespace + "Relationships")
        {
            throw new InvalidDataException(
                "The root relationships document has the wrong root element.");
        }

        List<Relationship> relationships = relationshipsRoot
            .Elements(relationshipsNamespace + "Relationship")
            .Select(ParseRelationship)
            .ToList();
        if (relationships.Count
            != relationshipsRoot.Elements().Count())
        {
            throw new InvalidDataException(
                "The root relationships document contains an unknown child element.");
        }

        Relationship[] coreRelationships = relationships
            .Where(
                relationship => StringComparer.Ordinal.Equals(
                    relationship.Type,
                    CorePropertiesRelationshipType))
            .ToArray();
        if (coreRelationships.Length != 1)
        {
            throw new InvalidDataException(
                $"The package must contain exactly one core-properties relationship; found {coreRelationships.Length}.");
        }

        string oldCorePropertiesEntryName =
            RelationshipTargetToEntryName(coreRelationships[0].Target);
        RequireSingleEntry(entries, oldCorePropertiesEntryName);

        if (!StringComparer.OrdinalIgnoreCase.Equals(
                oldCorePropertiesEntryName,
                CanonicalCorePropertiesEntryName)
            && entries.Any(
                entry => StringComparer.OrdinalIgnoreCase.Equals(
                    entry.Name,
                    CanonicalCorePropertiesEntryName)))
        {
            throw new InvalidDataException(
                $"The canonical core-properties path is already occupied: {CanonicalCorePropertiesEntryName}");
        }

        List<Relationship> canonicalRelationships = relationships
            .Select(
                relationship =>
                    StringComparer.Ordinal.Equals(
                        relationship.Type,
                        CorePropertiesRelationshipType)
                        ? relationship with
                        {
                            Target = "/" + CanonicalCorePropertiesEntryName,
                        }
                        : relationship)
            .OrderBy(relationship => relationship.Type, StringComparer.Ordinal)
            .ThenBy(relationship => relationship.Target, StringComparer.Ordinal)
            .ThenBy(
                relationship => relationship.TargetMode ?? String.Empty,
                StringComparer.Ordinal)
            .ToList();

        HashSet<string> relationshipIds = new(StringComparer.Ordinal);
        XElement canonicalRelationshipsRoot = new(
            relationshipsNamespace + "Relationships");
        foreach (Relationship relationship in canonicalRelationships)
        {
            string relationshipId = CreateRelationshipId(relationship);
            if (!relationshipIds.Add(relationshipId))
            {
                throw new InvalidDataException(
                    "Two package relationships produced the same canonical ID.");
            }

            XElement element = new(
                relationshipsNamespace + "Relationship",
                new XAttribute("Type", relationship.Type),
                new XAttribute("Target", relationship.Target),
                new XAttribute("Id", relationshipId));
            if (relationship.TargetMode is not null)
            {
                element.Add(
                    new XAttribute(
                        "TargetMode",
                        relationship.TargetMode));
            }

            canonicalRelationshipsRoot.Add(element);
        }

        byte[] canonicalRelationshipsBytes = WriteXml(
            new XDocument(
                new XDeclaration("1.0", "utf-8", null),
                canonicalRelationshipsRoot));

        InputEntry contentTypesEntry = entries.Single(
            entry => StringComparer.Ordinal.Equals(
                entry.Name,
                ContentTypesEntryName));
        byte[] contentTypesBytes = ReadAllBytes(contentTypesEntry.Entry);
        byte[] canonicalContentTypesBytes = NormalizeContentTypes(
            contentTypesBytes,
            oldCorePropertiesEntryName);

        return new NormalizedMetadata(
            oldCorePropertiesEntryName,
            canonicalRelationshipsBytes,
            canonicalContentTypesBytes);
    }

    private static Relationship ParseRelationship(XElement element)
    {
        string type = RequiredAttribute(element, "Type");
        string target = RequiredAttribute(element, "Target");
        string? targetMode = (string?)element.Attribute("TargetMode");
        if (element.Attributes().Any(
                attribute =>
                    attribute.Name.LocalName is not (
                        "Type" or "Target" or "Id" or "TargetMode")))
        {
            throw new InvalidDataException(
                "A root package relationship contains an unknown attribute.");
        }

        _ = RequiredAttribute(element, "Id");
        return new Relationship(type, target, targetMode);
    }

    private static string RequiredAttribute(
        XElement element,
        string localName)
    {
        string? value = (string?)element.Attribute(localName);
        if (String.IsNullOrEmpty(value))
        {
            throw new InvalidDataException(
                $"A package relationship is missing {localName}.");
        }

        return value;
    }

    private static string RelationshipTargetToEntryName(string target)
    {
        string entryName = target.StartsWith("/", StringComparison.Ordinal)
            ? target[1..]
            : target;
        ValidateEntryName(entryName);
        return entryName;
    }

    private static string CreateRelationshipId(Relationship relationship)
    {
        byte[] input = Encoding.UTF8.GetBytes(
            String.Join(
                '\0',
                relationship.Type,
                relationship.Target,
                relationship.TargetMode ?? String.Empty));
        byte[] digest = SHA256.HashData(input);
        return "R" + Convert.ToHexString(digest.AsSpan(0, 12));
    }

    private static byte[] NormalizeContentTypes(
        byte[] source,
        string oldCorePropertiesEntryName)
    {
        XDocument document = LoadXml(source, ContentTypesEntryName);
        XNamespace contentTypesNamespace =
            "http://schemas.openxmlformats.org/package/2006/content-types";
        XElement root =
            document.Root
            ?? throw new InvalidDataException(
                "The content-types document has no root element.");
        if (root.Name != contentTypesNamespace + "Types")
        {
            throw new InvalidDataException(
                "The content-types document has the wrong root element.");
        }

        string oldPartName = "/" + oldCorePropertiesEntryName;
        string canonicalPartName = "/" + CanonicalCorePropertiesEntryName;
        bool changed = false;
        foreach (XElement element in root.Elements(
                     contentTypesNamespace + "Override"))
        {
            XAttribute? partName = element.Attribute("PartName");
            if (partName is not null
                && StringComparer.Ordinal.Equals(
                    partName.Value,
                    oldPartName))
            {
                partName.Value = canonicalPartName;
                changed = true;
            }
        }

        return changed ? WriteXml(document) : source;
    }

    private static XDocument LoadXml(byte[] bytes, string entryName)
    {
        try
        {
            using MemoryStream stream = new(bytes, writable: false);
            return XDocument.Load(
                stream,
                LoadOptions.PreserveWhitespace);
        }
        catch (XmlException exception)
        {
            throw new InvalidDataException(
                $"The package entry {entryName} is not valid XML.",
                exception);
        }
    }

    private static byte[] WriteXml(XDocument document)
    {
        using MemoryStream stream = new();
        XmlWriterSettings settings = new()
        {
            Encoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
            Indent = true,
            NewLineChars = "\n",
            NewLineHandling = NewLineHandling.Replace,
            OmitXmlDeclaration = false,
            CloseOutput = false,
        };
        using (XmlWriter writer = XmlWriter.Create(stream, settings))
        {
            document.Save(writer);
        }

        return stream.ToArray();
    }

    private static byte[] ReadAllBytes(ZipArchiveEntry entry)
    {
        using Stream source = entry.Open();
        using MemoryStream destination = new(
            entry.Length <= Int32.MaxValue
                ? checked((int)entry.Length)
                : 0);
        source.CopyTo(destination);
        return destination.ToArray();
    }

    private static void WriteNormalizedEntries(
        IReadOnlyList<InputEntry> inputEntries,
        NormalizedMetadata metadata,
        ZipArchive outputArchive)
    {
        IEnumerable<OutputEntry> outputEntries = inputEntries
            .Select(
                input =>
                {
                    string outputName = StringComparer.Ordinal.Equals(
                        input.Name,
                        metadata.OldCorePropertiesEntryName)
                        ? CanonicalCorePropertiesEntryName
                        : input.Name;
                    byte[]? replacement = StringComparer.Ordinal.Equals(
                        input.Name,
                        RootRelationshipsEntryName)
                        ? metadata.RootRelationships
                        : StringComparer.Ordinal.Equals(
                            input.Name,
                            ContentTypesEntryName)
                            ? metadata.ContentTypes
                            : null;
                    return new OutputEntry(outputName, input.Entry, replacement);
                })
            .OrderBy(output => output.Name, StringComparer.Ordinal);

        HashSet<string> outputNames = new(StringComparer.OrdinalIgnoreCase);
        foreach (OutputEntry output in outputEntries)
        {
            if (!outputNames.Add(output.Name))
            {
                throw new InvalidDataException(
                    $"Normalization produced a duplicate entry: {output.Name}");
            }

            ZipArchiveEntry destination = outputArchive.CreateEntry(
                output.Name,
                CompressionLevel.SmallestSize);
            destination.LastWriteTime = CanonicalTimestamp;
            destination.ExternalAttributes =
                CanonicalFileExternalAttributes;

            using Stream destinationStream = destination.Open();
            if (output.Replacement is not null)
            {
                destinationStream.Write(output.Replacement);
            }
            else
            {
                using Stream sourceStream = output.Source.Open();
                sourceStream.CopyTo(destinationStream);
            }
        }
    }

    private static void CompareProductPayload(
        string expectedPath,
        string actualPath)
    {
        string work = Path.Combine(
            Path.GetTempPath(),
            $"baml-nuget-compare-{Guid.NewGuid():N}");
        Directory.CreateDirectory(work);
        string expectedNormalized = Path.Combine(work, "expected.nupkg");
        string actualNormalized = Path.Combine(work, "actual.nupkg");
        try
        {
            Normalize(
                Path.GetFullPath(expectedPath),
                expectedNormalized,
                allowRepositorySignature: true);
            Normalize(
                Path.GetFullPath(actualPath),
                actualNormalized,
                allowRepositorySignature: true);
            if (!FilesEqual(expectedNormalized, actualNormalized))
            {
                throw new InvalidDataException(
                    "the existing immutable version has different product-owned content");
            }
        }
        finally
        {
            Directory.Delete(work, recursive: true);
        }
    }

    private static bool FilesEqual(string leftPath, string rightPath)
    {
        FileInfo leftInfo = new(leftPath);
        FileInfo rightInfo = new(rightPath);
        if (leftInfo.Length != rightInfo.Length)
        {
            return false;
        }

        using FileStream left = File.OpenRead(leftPath);
        using FileStream right = File.OpenRead(rightPath);
        byte[] leftBuffer = new byte[1 << 20];
        byte[] rightBuffer = new byte[leftBuffer.Length];
        long remaining = leftInfo.Length;
        while (remaining > 0)
        {
            int chunkLength = (int)Math.Min(leftBuffer.Length, remaining);
            Span<byte> leftChunk = leftBuffer.AsSpan(0, chunkLength);
            Span<byte> rightChunk = rightBuffer.AsSpan(0, chunkLength);
            left.ReadExactly(leftChunk);
            right.ReadExactly(rightChunk);
            if (!leftChunk.SequenceEqual(rightChunk))
            {
                return false;
            }
            remaining -= chunkLength;
        }
        return true;
    }

    private static void RunSelfTests()
    {
        string work = Path.Combine(
            Path.GetTempPath(),
            $"baml-nuget-self-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(work);
        try
        {
            string expected = Path.Combine(work, "expected.nupkg");
            string signed = Path.Combine(work, "signed.nupkg");
            WriteTestPackage(expected, signature: false, mutation: null);
            WriteTestPackage(signed, signature: true, mutation: null);
            CompareProductPayload(expected, signed);

            foreach (string mutation in new[]
                     {
                         "managed-assembly",
                         "native-asset",
                         "nuspec-version",
                         "nuspec-dependency",
                     })
            {
                string changed = Path.Combine(work, $"{mutation}.nupkg");
                WriteTestPackage(
                    changed,
                    signature: true,
                    mutation: mutation);
                bool rejected = false;
                try
                {
                    CompareProductPayload(expected, changed);
                }
                catch (InvalidDataException)
                {
                    rejected = true;
                }
                if (!rejected)
                {
                    throw new InvalidOperationException(
                        $"payload mutation was not rejected: {mutation}");
                }
            }
        }
        finally
        {
            Directory.Delete(work, recursive: true);
        }
    }

    private static void WriteTestPackage(
        string path,
        bool signature,
        string? mutation)
    {
        const string contentTypes =
            """
            <?xml version="1.0" encoding="utf-8"?>
            <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
              <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml" />
              <Default Extension="psmdcp" ContentType="application/vnd.openxmlformats-package.core-properties+xml" />
              <Default Extension="nuspec" ContentType="application/octet" />
              <Default Extension="dll" ContentType="application/octet" />
              <Default Extension="so" ContentType="application/octet" />
            </Types>
            """;
        const string relationships =
            """
            <?xml version="1.0" encoding="utf-8"?>
            <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
              <Relationship Type="http://schemas.microsoft.com/packaging/2010/07/manifest" Target="/baml-bridge.nuspec" Id="manifest" />
              <Relationship Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="/package/services/metadata/core-properties/random.psmdcp" Id="properties" />
            </Relationships>
            """;
        string version = mutation == "nuspec-version" ? "1.2.4" : "1.2.3";
        string dependency = mutation == "nuspec-dependency"
            ? "[3.36.0,4.0.0)"
            : "[3.35.1,4.0.0)";
        string nuspec =
            $"""
             <?xml version="1.0" encoding="utf-8"?>
             <package xmlns="http://schemas.microsoft.com/packaging/2013/05/nuspec.xsd">
               <metadata>
                 <id>baml-bridge</id>
                 <version>{version}</version>
                 <authors>BoundaryML</authors>
                 <description>comparison fixture</description>
                 <dependencies>
                   <group targetFramework="net10.0">
                     <dependency id="Google.Protobuf" version="{dependency}" />
                   </group>
                 </dependencies>
               </metadata>
             </package>
             """;

        using FileStream stream = new(
            path,
            FileMode.CreateNew,
            FileAccess.ReadWrite,
            FileShare.None);
        using ZipArchive archive = new(
            stream,
            ZipArchiveMode.Create,
            leaveOpen: false,
            entryNameEncoding: Encoding.UTF8);
        AddTestEntry(archive, ContentTypesEntryName, contentTypes);
        AddTestEntry(archive, RootRelationshipsEntryName, relationships);
        AddTestEntry(
            archive,
            "package/services/metadata/core-properties/random.psmdcp",
            "<core>product-owned</core>");
        AddTestEntry(archive, "baml-bridge.nuspec", nuspec);
        AddTestEntry(
            archive,
            "lib/net10.0/Baml.Bridge.dll",
            mutation == "managed-assembly" ? "changed-managed" : "managed");
        AddTestEntry(
            archive,
            "runtimes/linux-x64/native/libbridge_cffi.so",
            mutation == "native-asset" ? "changed-native" : "native");
        if (signature)
        {
            AddTestEntry(archive, SignatureEntryName, "repository-signature");
        }
    }

    private static void AddTestEntry(
        ZipArchive archive,
        string name,
        string content)
    {
        ZipArchiveEntry entry = archive.CreateEntry(
            name,
            CompressionLevel.SmallestSize);
        entry.LastWriteTime = CanonicalTimestamp;
        entry.ExternalAttributes = CanonicalFileExternalAttributes;
        using StreamWriter writer = new(
            entry.Open(),
            new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
        writer.Write(content);
    }

    private sealed record InputEntry(
        string Name,
        ZipArchiveEntry Entry);

    private sealed record OutputEntry(
        string Name,
        ZipArchiveEntry Source,
        byte[]? Replacement);

    private sealed record Relationship(
        string Type,
        string Target,
        string? TargetMode);

    private sealed record NormalizedMetadata(
        string OldCorePropertiesEntryName,
        byte[] RootRelationships,
        byte[] ContentTypes);
}
