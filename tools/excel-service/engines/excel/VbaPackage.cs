using System.IO.Compression;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;

namespace Touchstone.Excel;

/// <summary>
/// Checks that a VBA case's workbook is exactly what CalcDoc builds from the case's sheets and modules
/// (rw14 finding 3). Same checks as tools/excel-service/vba_package.py; keep both in step with
/// crates/calc-doc/src/vba_write.rs.
/// </summary>
static class VbaPackage
{
    const int MaxProject = 8 * 1024 * 1024, MaxPart = 64 * 1024;
    const uint End = 0xFFFFFFFE, None = 0xFFFFFFFF;
    const string ProjectName = "VBAProject", ProjectId = "{CB4A6FD3-4DB2-4698-915C-D69261AB7550}";
    const string Rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
    const string XmlHead = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
    const string SheetType = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";

    static readonly Dictionary<string, string> ContentTypes = new()
    {
        ["/xl/workbook.xml"] = "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
        ["/xl/styles.xml"] = "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        ["/xl/sharedStrings.xml"] = "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
        ["/xl/vbaProject.bin"] = "application/vnd.ms-office.vbaProject",
    };
    static readonly Dictionary<string, string> Defaults = new()
    {
        ["rels"] = "application/vnd.openxmlformats-package.relationships+xml",
        ["xml"] = "application/xml",
    };
    // null: a worksheet part; otherwise the one allowed target.
    static readonly Dictionary<string, string> RelationshipTypes = new()
    {
        [Rel + "worksheet"] = null, [Rel + "styles"] = "styles.xml", [Rel + "sharedStrings"] = "sharedStrings.xml",
        ["http://schemas.microsoft.com/office/2006/relationships/vbaProject"] = "vbaProject.bin",
    };
    const string Styles = XmlHead
        + "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><fonts count=\"1\"><font>"
        + "<sz val=\"11\"/><color auto=\"1\"/><name val=\"Calibri\"/><family val=\"2\"/><scheme val=\"minor\"/></font></fonts>"
        + "<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/>"
        + "</fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"
        + "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>"
        + "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>"
        + "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>";
    const string SharedStrings = XmlHead
        + "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"0\" uniqueCount=\"0\"/>";

    static string SheetXml(int index) => XmlHead
        + "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">"
        + $"<sheetPr codeName=\"Sheet{index}\"/><sheetData/></worksheet>";

    static RefusedVbaException Refused(string detail) => new("invalid VBA workbook: " + detail);

    public static void Verify(byte[] data, Dictionary<string, object> probe)
    {
        var sheets = ((List<object>)probe["sheets"]).Select(s => (string)((Dictionary<string, object>)s)["name"]).ToList();
        var modules = ProjectModules((List<object>)probe["modules"], sheets.Count);
        var parts = ReadPackage(data, sheets.Count);
        CheckParts(parts, sheets);
        CheckProject(parts["xl/vbaProject.bin"], modules);
    }

    static Dictionary<string, byte[]> ReadPackage(byte[] data, int sheetCount)
    {
        ZipArchive archive;
        try { archive = new ZipArchive(new MemoryStream(data), ZipArchiveMode.Read); }
        catch (InvalidDataException) { throw Refused("not a package"); }
        var expected = new List<string>
        {
            "[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/_rels/workbook.xml.rels",
            "xl/styles.xml", "xl/sharedStrings.xml", "xl/vbaProject.bin",
        };
        for (int i = 1; i <= sheetCount; i++) expected.Add($"xl/worksheets/sheet{i}.xml");
        if (!archive.Entries.Select(e => e.FullName).Order(StringComparer.Ordinal)
                .SequenceEqual(expected.Order(StringComparer.Ordinal)))
            throw Refused("unexpected package parts");
        var parts = new Dictionary<string, byte[]>();
        foreach (var entry in archive.Entries)
        {
            long limit = entry.FullName == "xl/vbaProject.bin" ? MaxProject : MaxPart;
            if (entry.Length > limit) throw Refused("oversized package part");
            using var stream = entry.Open();
            var buffer = new MemoryStream();
            var chunk = new byte[8192];
            int n;
            while ((n = stream.Read(chunk, 0, chunk.Length)) > 0)
            {
                buffer.Write(chunk, 0, n);
                if (buffer.Length > limit) throw Refused("oversized package part");
            }
            if (buffer.Length != entry.Length) throw Refused("package part size");
            parts[entry.FullName] = buffer.ToArray();
        }
        return parts;
    }

    static XElement Xml(byte[] body, string what)
    {
        var settings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Prohibit, XmlResolver = null };
        try
        {
            using var reader = XmlReader.Create(new StringReader(new UTF8Encoding(false, true).GetString(body)), settings);
            return XDocument.Load(reader).Root;
        }
        catch (Exception e) when (e is XmlException or DecoderFallbackException)
        {
            throw Refused(what + " is not plain XML");
        }
    }

    static string Attr(XElement e, string name) => (string)e.Attribute(name);
    static bool AttrsAre(XElement e, params string[] names) =>
        e.Attributes().Select(a => a.Name.NamespaceName.Length == 0 ? a.Name.LocalName : a.Name.ToString())
            .Order(StringComparer.Ordinal).SequenceEqual(names.Order(StringComparer.Ordinal));

    static void CheckParts(Dictionary<string, byte[]> parts, List<string> sheets)
    {
        int count = sheets.Count;
        if (!parts["xl/styles.xml"].AsSpan().SequenceEqual(Encoding.UTF8.GetBytes(Styles))
            || !parts["xl/sharedStrings.xml"].AsSpan().SequenceEqual(Encoding.UTF8.GetBytes(SharedStrings)))
            throw Refused("styles or shared strings differ from the blank workbook");
        for (int i = 1; i <= count; i++)
            if (!parts[$"xl/worksheets/sheet{i}.xml"].AsSpan().SequenceEqual(Encoding.UTF8.GetBytes(SheetXml(i))))
                throw Refused("a worksheet is not an empty sheet");
        var present = parts.Keys.Select(k => "/" + k).ToHashSet();
        var seen = new HashSet<string>();
        foreach (var element in Xml(parts["[Content_Types].xml"], "content types").Elements())
        {
            var name = element.Name.LocalName;
            if (name == "Default" && AttrsAre(element, "Extension", "ContentType"))
            {
                if (!Defaults.TryGetValue(Attr(element, "Extension"), out var kind) || kind != Attr(element, "ContentType"))
                    throw Refused("unexpected default content type");
            }
            else if (name == "Override" && AttrsAre(element, "PartName", "ContentType"))
            {
                var part = Attr(element, "PartName");
                var wanted = ContentTypes.GetValueOrDefault(part)
                    ?? (Regex.IsMatch(part, @"\A/xl/worksheets/sheet[1-9][0-9]*\.xml\z") ? SheetType : null);
                if (Attr(element, "ContentType") != wanted || !present.Contains(part) || !seen.Add(part))
                    throw Refused("unexpected content type override");
            }
            else throw Refused("unexpected content types entry");
        }
        present.ExceptWith(new[] { "/[Content_Types].xml", "/_rels/.rels", "/xl/_rels/workbook.xml.rels" });
        if (!seen.SetEquals(present)) throw Refused("content types do not cover the package");
        var rootRels = Xml(parts["_rels/.rels"], "relationships").Elements().ToList();
        if (rootRels.Count != 1 || !AttrsAre(rootRels[0], "Id", "Type", "Target")
            || Attr(rootRels[0], "Type") != Rel + "officeDocument" || Attr(rootRels[0], "Target") != "xl/workbook.xml")
            throw Refused("unexpected package relationships");
        var targets = new Dictionary<string, (string kind, string target)>();
        foreach (var element in Xml(parts["xl/_rels/workbook.xml.rels"], "relationships").Elements())
        {
            var kind = Attr(element, "Type");
            var target = Attr(element, "Target");
            if (element.Name.LocalName != "Relationship" || !AttrsAre(element, "Id", "Type", "Target")
                || kind == null || target == null || !RelationshipTypes.TryGetValue(kind, out var fixedTarget)
                || fixedTarget != null && target != fixedTarget
                || fixedTarget == null && !Regex.IsMatch(target, @"\Aworksheets/sheet[1-9][0-9]*\.xml\z")
                || !parts.ContainsKey("xl/" + target) || !targets.TryAdd(Attr(element, "Id"), (kind, target)))
                throw Refused("unexpected workbook relationship");
        }
        if (!targets.Values.Where(t => t.kind.EndsWith("/worksheet")).Select(t => t.target).Order(StringComparer.Ordinal)
                .SequenceEqual(Enumerable.Range(1, count).Select(i => $"worksheets/sheet{i}.xml").Order(StringComparer.Ordinal)))
            throw Refused("worksheet relationships do not match the sheets");
        if (targets.Values.Count(t => t.kind.EndsWith("/vbaProject")) != 1)
            throw Refused("one vbaProject relationship is required");
        var workbook = Xml(parts["xl/workbook.xml"], "workbook");
        var children = workbook.Elements().ToList();
        if (workbook.Name.LocalName != "workbook" || !children.Select(e => e.Name.LocalName).SequenceEqual(new[] { "workbookPr", "sheets", "calcPr" })
            || !AttrsAre(children[0], "codeName") || Attr(children[0], "codeName") != "ThisWorkbook"
            || children[2].Attributes().Any(a => a.Name.LocalName != "calcId") || children[1].Elements().Count() != count)
            throw Refused("unexpected workbook.xml");
        XNamespace r = Rel.TrimEnd('/');
        int index = 0;
        foreach (var element in children[1].Elements())
        {
            index++;
            var rid = (string)element.Attribute(r + "id");
            var target = rid != null && targets.TryGetValue(rid, out var t) ? t.target : null;
            if (element.Name.LocalName != "sheet" || !AttrsAre(element, "name", "sheetId", "{" + r.NamespaceName + "}id")
                || Attr(element, "name") != sheets[index - 1] || Attr(element, "sheetId") != index.ToString()
                || target != $"worksheets/sheet{index}.xml")
                throw Refused("workbook sheets do not match the case");
        }
    }

    // --- the project, rebuilt from the modules ----------------------------------------------------

    record Module(string Name, string Kind, string Source);

    static List<Module> ProjectModules(List<object> items, int sheetCount)
    {
        var modules = new List<Module>();
        foreach (Dictionary<string, object> module in items)
        {
            var name = (string)module["name"];
            modules.Add(new Module(name, (string)module["kind"] == "standard" ? "Standard"
                : name == "ThisWorkbook" ? "Workbook" : "Worksheet", (string)module["source"]));
        }
        var documents = new List<(string kind, string name)> { ("Workbook", "ThisWorkbook") };
        for (int i = 1; i <= sheetCount; i++) documents.Add(("Worksheet", $"Sheet{i}"));
        foreach (var (kind, name) in documents)
            if (!modules.Any(m => m.Kind == kind && m.Name == name)) modules.Add(new Module(name, kind, ""));
        foreach (var module in modules)
            if (module.Kind != "Standard" && !documents.Contains((module.Kind, module.Name)))
                throw Refused("a document module does not name ThisWorkbook or a sheet code name");
        return modules;
    }

    static byte[] EncodedSource(Module module)
    {
        var normalized = module.Source.Replace("\r\n", "\n").Replace("\r", "\n");
        var text = $"Attribute VB_Name = \"{module.Name}\"\r\n";
        string baseId = module.Kind switch
        {
            "Class" => "FCFB3D2A-A0FA-1068-A738-08002B3371B5",
            "Workbook" => "00020819-0000-0000-C000-000000000046",
            "Worksheet" => "00020820-0000-0000-C000-000000000046",
            _ => null,
        };
        if (baseId != null)
        {
            var flag = module.Kind == "Class" ? "False" : "True";
            text += $"Attribute VB_Base = \"0{{{baseId}}}\"\r\nAttribute VB_GlobalNameSpace = False\r\n"
                + $"Attribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = {flag}\r\n"
                + $"Attribute VB_Exposed = {flag}\r\nAttribute VB_TemplateDerived = False\r\n"
                + $"Attribute VB_Customizable = {flag}\r\n";
        }
        try
        {
            Encoding.RegisterProvider(CodePagesEncodingProvider.Instance);
            return Encoding.GetEncoding(1252, EncoderFallback.ExceptionFallback, DecoderFallback.ExceptionFallback)
                .GetBytes(text + normalized.Replace("\n", "\r\n"));
        }
        catch (EncoderFallbackException) { throw Refused("module source is not Windows-1252"); }
    }

    static void Record(List<byte> dir, int id, byte[] body = null)
    {
        body ??= Array.Empty<byte>();
        dir.AddRange(BitConverter.GetBytes((ushort)id));
        dir.AddRange(BitConverter.GetBytes((uint)body.Length));
        dir.AddRange(body);
    }

    static byte[] U32(uint v) => BitConverter.GetBytes(v);
    static byte[] U16(ushort v) => BitConverter.GetBytes(v);
    static byte[] Utf16(string s) => Encoding.Unicode.GetBytes(s);
    static byte[] Ascii(string s) => Encoding.ASCII.GetBytes(s);

    static byte[] Directory(List<Module> modules)
    {
        var d = new List<byte>();
        Record(d, 0x01, U32(3)); Record(d, 0x4A, U32(6)); Record(d, 0x02, U32(0x0409)); Record(d, 0x14, U32(0x0409));
        Record(d, 0x03, U16(1252)); Record(d, 0x04, Ascii(ProjectName));
        foreach (var id in new[] { 0x05, 0x40, 0x06, 0x3D }) Record(d, id);
        Record(d, 0x07, new byte[4]); Record(d, 0x08, new byte[4]); Record(d, 0x09, U32(1));
        d.AddRange(U16(0));
        Record(d, 0x0C); Record(d, 0x3C);
        foreach (var (name, libid) in new[]
        {
            ("stdole", @"*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation"),
            ("Office", @"*\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.0#0#C:\Program Files\Common Files\Microsoft Shared\OFFICE16\MSO.DLL#Microsoft Office 16.0 Object Library"),
        })
        {
            Record(d, 0x16, Ascii(name)); Record(d, 0x3E, Utf16(name));
            Record(d, 0x0D, U32((uint)libid.Length).Concat(Ascii(libid)).Concat(new byte[6]).ToArray());
        }
        Record(d, 0x0F, U16((ushort)modules.Count)); Record(d, 0x13, new byte[2]);
        foreach (var module in modules)
        {
            Record(d, 0x19, Ascii(module.Name)); Record(d, 0x47, Utf16(module.Name));
            Record(d, 0x1A, Ascii(module.Name)); Record(d, 0x32, Utf16(module.Name));
            Record(d, 0x1C); Record(d, 0x48); Record(d, 0x31, new byte[4]); Record(d, 0x1E, new byte[4]);
            Record(d, 0x2C, new byte[2]); Record(d, module.Kind == "Standard" ? 0x21 : 0x22); Record(d, 0x2B);
        }
        Record(d, 0x10);
        return d.ToArray();
    }

    static string ProjectProperty(byte[] data)
    {
        byte key = unchecked((byte)Ascii(ProjectId).Sum(b => b));
        byte previous = key, earlier = 2, unencoded = key;
        var sb = new StringBuilder($"00{earlier:X2}{previous:X2}");
        foreach (var b in U32((uint)data.Length).Concat(data))
        {
            byte encoded = (byte)(b ^ (byte)(earlier + unencoded));
            sb.Append($"{encoded:X2}");
            earlier = previous; previous = encoded; unencoded = b;
        }
        return sb.ToString();
    }

    static byte[] ProjectStream(List<Module> modules)
    {
        var sb = new StringBuilder($"ID=\"{ProjectId}\"\r\n");
        foreach (var m in modules)
            sb.Append(m.Kind switch { "Standard" => "Module=", "Class" => "Class=", _ => "Document=" })
              .Append(m.Name).Append(m.Kind is "Standard" or "Class" ? "" : "/&H00000000").Append("\r\n");
        sb.Append($"Name=\"{ProjectName}\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\n");
        foreach (var (property, data) in new[] { ("CMG", new byte[4]), ("DPB", new byte[] { 0 }), ("GC", new byte[] { 0xFF }) })
            sb.Append($"{property}=\"{ProjectProperty(data)}\"\r\n");
        sb.Append("\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n");
        return Ascii(sb.ToString());
    }

    static byte[] NameMap(List<Module> modules)
    {
        var map = new List<byte>();
        foreach (var m in modules) { map.AddRange(Ascii(m.Name)); map.Add(0); map.AddRange(Utf16(m.Name)); map.AddRange(new byte[2]); }
        map.AddRange(new byte[2]);
        return map.ToArray();
    }

    static void CheckProject(byte[] blob, List<Module> modules)
    {
        var streams = ReadCompoundFile(blob);
        var plain = new Dictionary<string, byte[]>
        {
            ["PROJECT"] = ProjectStream(modules), ["PROJECTwm"] = NameMap(modules),
            ["VBA/_VBA_PROJECT"] = new byte[] { 0xCC, 0x61, 0xFF, 0xFF, 0, 1, 0 },
        };
        var packed = new Dictionary<string, byte[]> { ["VBA/dir"] = Directory(modules) };
        foreach (var m in modules) packed["VBA/" + m.Name] = EncodedSource(m);
        if (!streams.Keys.ToHashSet().SetEquals(plain.Keys.Concat(packed.Keys)))
            throw Refused("vbaProject.bin has unexpected or missing streams");
        foreach (var (path, body) in plain)
            if (!streams[path].AsSpan().SequenceEqual(body))
                throw Refused($"vbaProject.bin stream {path} differs from the authored project");
        foreach (var (path, body) in packed)
            if (!Decompress(streams[path]).AsSpan().SequenceEqual(body))
                throw Refused($"vbaProject.bin stream {path} differs from the authored source");
    }

    public static byte[] Decompress(byte[] data)
    {
        if (data.Length == 0 || data[0] != 1) throw Refused("bad compressed container");
        var output = new List<byte>();
        int at = 1;
        while (at < data.Length)
        {
            if (at + 2 > data.Length) throw Refused("truncated compressed chunk");
            int header = data[at] | data[at + 1] << 8;
            int end = at + (header & 0xFFF) + 3;
            if ((header >> 12 & 7) != 3 || end > data.Length) throw Refused("bad compressed chunk");
            if ((header & 0x8000) == 0)
            {
                if (end - at != 4098) throw Refused("bad raw chunk");
                for (int i = at + 2; i < end; i++) output.Add(data[i]);
            }
            else
            {
                int start = output.Count, pos = at + 2;
                while (pos < end)
                {
                    int flags = data[pos++];
                    for (int bit = 0; bit < 8 && pos < end; bit++)
                    {
                        if ((flags >> bit & 1) == 0) { output.Add(data[pos++]); continue; }
                        if (pos + 2 > end) throw Refused("truncated copy token");
                        int token = data[pos] | data[pos + 1] << 8;
                        pos += 2;
                        int distance = output.Count - start;
                        int bits = distance == 0 ? 4 : Math.Max(32 - System.Numerics.BitOperations.LeadingZeroCount((uint)(distance - 1)), 4);
                        int length = (token & (0xFFFF >> bits)) + 3;
                        int offset = (token >> (16 - bits)) + 1;
                        if (offset > distance) throw Refused("bad copy token");
                        for (int i = 0; i < length; i++) output.Add(output[^offset]);
                    }
                }
            }
            at = end;
            if (output.Count > MaxProject) throw Refused("oversized module stream");
        }
        return output.ToArray();
    }

    record Entry(string Name, byte Kind, uint Left, uint Right, uint Child, uint Start, ulong Size);

    static Dictionary<string, byte[]> ReadCompoundFile(byte[] blob)
    {
        const int Sector = 512;
        if (blob.Length < 1536 || blob.Length > MaxProject
            || !blob.AsSpan(0, 8).SequenceEqual(new byte[] { 0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1 }))
            throw Refused("vbaProject.bin is not a compound file");
        ushort U16At(int at) => BitConverter.ToUInt16(blob, at);
        uint U32At(int at) => BitConverter.ToUInt32(blob, at);
        if (U16At(26) != 3 || U16At(28) != 0xFFFE || U16At(30) != 9 || U16At(32) != 6)
            throw Refused("unsupported compound file layout");
        uint fatCount = U32At(44), dirStart = U32At(48), cutoff = U32At(56), miniStart = U32At(60),
            miniCount = U32At(64), difatStart = U32At(68), difatCount = U32At(72);
        int sectors = (blob.Length - Sector) / Sector;
        ReadOnlySpan<byte> SectorBytes(uint index)
        {
            if (index >= sectors) throw Refused("sector out of range");
            return blob.AsSpan(Sector + (int)index * Sector, Sector);
        }
        var difat = new List<uint>();
        for (int i = 0; i < 109; i++) difat.Add(U32At(76 + 4 * i));
        uint chainAt = difatStart;
        var chainSeen = new HashSet<uint>();
        for (uint i = 0; i < difatCount; i++)
        {
            if (!chainSeen.Add(chainAt) || chainAt >= sectors) throw Refused("bad DIFAT");
            var entries = new uint[128];
            var span = SectorBytes(chainAt);
            for (int k = 0; k < 128; k++) entries[k] = BitConverter.ToUInt32(span.Slice(4 * k, 4));
            difat.AddRange(entries.Take(127));
            chainAt = entries[127];
        }
        var fat = new List<uint>();
        foreach (var index in difat.Take((int)Math.Min(fatCount, (uint)difat.Count)))
        {
            var span = SectorBytes(index);
            for (int k = 0; k < 128; k++) fat.Add(BitConverter.ToUInt32(span.Slice(4 * k, 4)));
        }
        List<uint> ChainOf(uint start, List<uint> table)
        {
            var result = new List<uint>();
            var seen = new HashSet<uint>();
            while (start != End)
            {
                if (start >= table.Count || !seen.Add(start)) throw Refused("bad sector chain");
                result.Add(start);
                start = table[(int)start];
            }
            return result;
        }
        byte[] ReadChain(uint start, long size)
        {
            var body = new List<byte>();
            foreach (var index in ChainOf(start, fat)) body.AddRange(SectorBytes(index).ToArray());
            if (body.Count < size) throw Refused("short stream");
            return body.Take((int)size).ToArray();
        }
        var directory = ReadChain(dirStart, ChainOf(dirStart, fat).Count * Sector);
        var table = new List<Entry>();
        for (int at = 0; at + 128 <= directory.Length; at += 128)
        {
            int length = BitConverter.ToUInt16(directory, at + 64);
            byte kind = directory[at + 66];
            if (kind != 0 && (length < 2 || length > 64 || length % 2 != 0)) throw Refused("bad directory entry name");
            string name;
            try { name = kind == 0 ? "" : new UnicodeEncoding(false, false, true).GetString(directory, at, length - 2); }
            catch (ArgumentException) { throw Refused("bad directory entry name"); }
            table.Add(new Entry(name, kind, BitConverter.ToUInt32(directory, at + 68), BitConverter.ToUInt32(directory, at + 72),
                BitConverter.ToUInt32(directory, at + 76), BitConverter.ToUInt32(directory, at + 116),
                BitConverter.ToUInt64(directory, at + 120)));
        }
        if (table.Count == 0 || table[0].Kind != 5) throw Refused("no root entry");
        var miniFat = new List<uint>();
        if (miniCount != 0)
        {
            var body = ReadChain(miniStart, ChainOf(miniStart, fat).Count * Sector);
            for (int k = 0; k + 4 <= body.Length; k += 4) miniFat.Add(BitConverter.ToUInt32(body, k));
        }
        var miniStream = table[0].Size != 0 ? ReadChain(table[0].Start, (long)table[0].Size) : Array.Empty<byte>();
        byte[] StreamOf(Entry entry)
        {
            if (entry.Size >= cutoff) return ReadChain(entry.Start, (long)entry.Size);
            var body = new List<byte>();
            foreach (var index in ChainOf(entry.Start, miniFat))
            {
                if ((long)index * 64 + 64 > miniStream.Length) throw Refused("short mini stream");
                body.AddRange(miniStream.AsSpan((int)index * 64, 64).ToArray());
            }
            if (body.Count < (long)entry.Size) throw Refused("short mini stream");
            return body.Take((int)entry.Size).ToArray();
        }
        var found = new Dictionary<string, byte[]>();
        var visited = new HashSet<uint> { 0 };
        void Walk(uint index, string prefix)
        {
            var stack = new Stack<uint>();
            stack.Push(index);
            while (stack.Count > 0)
            {
                var at = stack.Pop();
                if (at == None) continue;
                if (at >= table.Count || !visited.Add(at)) throw Refused("bad directory tree");
                var entry = table[(int)at];
                var path = prefix + entry.Name;
                if (entry.Kind == 2)
                {
                    if (!found.TryAdd(path, StreamOf(entry))) throw Refused("duplicate stream");
                }
                else if (entry.Kind == 1) Walk(entry.Child, path + "/");
                else throw Refused("unexpected directory entry");
                stack.Push(entry.Left);
                stack.Push(entry.Right);
            }
        }
        Walk(table[0].Child, "");
        if (table.Count(e => e.Kind != 0) != visited.Count) throw Refused("unreachable directory entries");
        return found;
    }
}
