using System.IO.Compression;
using System.Text;

namespace Touchstone.Excel;

/// <summary>
/// Builds the .xlsm for a VBA case from the modules and sheets the policy checked (dc-gqzuw). No uploaded
/// package is ever read: the worker writes its own, so a parser differential on a caller's compound file or
/// compressed stream cannot matter. Ported from crates/calc-doc/src/vba_write.rs, blank.rs and
/// xls/cfb_write.rs; the tests compare its vbaProject.bin with CalcDoc's.
/// </summary>
static class VbaPackage
{
    const string ProjectName = "VBAProject", ProjectId = "{CB4A6FD3-4DB2-4698-915C-D69261AB7550}";
    const string Head = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
    const string Spreadsheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.";
    const string RelBase = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
    const int MaxProject = 8 * 1024 * 1024;

    sealed record Module(string Name, string Kind, string Source);   // Kind: Standard | Workbook | Worksheet

    public static byte[] Build(VbaCase probe)
    {
        var sheets = probe.Sheets.Select(s => s.Name).ToList();
        var modules = probe.Modules.Select(m => new Module(m.Name,
            m.Kind == "standard" ? "Standard" : m.Name == "ThisWorkbook" ? "Workbook" : "Worksheet", m.Source)).ToList();
        var documents = new List<(string kind, string name)> { ("Workbook", "ThisWorkbook") };
        for (int i = 1; i <= sheets.Count; i++) documents.Add(("Worksheet", $"Sheet{i}"));
        foreach (var m in modules.Where(m => m.Kind != "Standard"))
            if (!documents.Contains((m.Kind, m.Name))) throw new RefusedVbaException("a document module does not name ThisWorkbook or a sheet code name");
        foreach (var (kind, name) in documents)
            if (!modules.Any(m => m.Kind == kind && m.Name == name)) modules.Add(new Module(name, kind, ""));
        return Package(sheets, WriteProject(modules));
    }

    // --- the package ------------------------------------------------------------------------------------

    static string Escaped(string text) => text.Replace("&", "&amp;").Replace("<", "&lt;").Replace(">", "&gt;").Replace("\"", "&quot;");

    static byte[] Package(List<string> sheets, byte[] project)
    {
        var contentTypes = Head + "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">"
            + "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>"
            + "<Default Extension=\"xml\" ContentType=\"application/xml\"/>"
            + "<Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.ms-excel.sheet.macroEnabled.main+xml\"/>"
            + $"<Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"{Spreadsheet}worksheet+xml\"/>"
            + $"<Override PartName=\"/xl/styles.xml\" ContentType=\"{Spreadsheet}styles+xml\"/>"
            + $"<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"{Spreadsheet}sharedStrings+xml\"/>";
        var relationships = Head + "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
            + $"<Relationship Id=\"rId1\" Type=\"{RelBase}worksheet\" Target=\"worksheets/sheet1.xml\"/>"
            + $"<Relationship Id=\"rId2\" Type=\"{RelBase}styles\" Target=\"styles.xml\"/>"
            + $"<Relationship Id=\"rId3\" Type=\"{RelBase}sharedStrings\" Target=\"sharedStrings.xml\"/>";
        var sheetElements = "<sheets>";
        var worksheets = new List<(string path, string xml)>();
        for (int index = 0; index < sheets.Count; index++)
        {
            int id = index + 1, rel = index == 0 ? 1 : index + 4;
            var path = $"worksheets/sheet{id}.xml";
            sheetElements += $"<sheet name=\"{Escaped(sheets[index])}\" sheetId=\"{id}\" r:id=\"rId{rel}\"/>";
            if (index > 0)
            {
                contentTypes += $"<Override PartName=\"/xl/{path}\" ContentType=\"{Spreadsheet}worksheet+xml\"/>";
                relationships += $"<Relationship Id=\"rId{rel}\" Type=\"{RelBase}worksheet\" Target=\"{path}\"/>";
            }
            worksheets.Add(("xl/" + path, Head + "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">"
                + $"<sheetPr codeName=\"Sheet{id}\"/><sheetData/></worksheet>"));
        }
        sheetElements += "</sheets>";
        contentTypes += "<Override PartName=\"/xl/vbaProject.bin\" ContentType=\"application/vnd.ms-office.vbaProject\"/></Types>";
        relationships += "<Relationship Id=\"rId4\" Type=\"http://schemas.microsoft.com/office/2006/relationships/vbaProject\" Target=\"vbaProject.bin\"/></Relationships>";
        var workbook = Head + "<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"" + RelBase.TrimEnd('/') + "\">"
            + "<workbookPr codeName=\"ThisWorkbook\"/>" + sheetElements + "<calcPr calcId=\"191029\"/></workbook>";
        var packageRelationships = Head + "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">"
            + $"<Relationship Id=\"rId1\" Type=\"{RelBase}officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>";
        var styles = Head + "<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">"
            + "<fonts count=\"1\"><font><sz val=\"11\"/><color auto=\"1\"/><name val=\"Calibri\"/><family val=\"2\"/><scheme val=\"minor\"/></font></fonts>"
            + "<fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>"
            + "<borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>"
            + "<cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>"
            + "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>"
            + "<cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>";
        var shared = Head + "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"0\" uniqueCount=\"0\"/>";
        var parts = new List<(string name, byte[] body)>
        {
            ("[Content_Types].xml", Encoding.UTF8.GetBytes(contentTypes)),
            ("_rels/.rels", Encoding.UTF8.GetBytes(packageRelationships)),
            ("xl/workbook.xml", Encoding.UTF8.GetBytes(workbook)),
            ("xl/_rels/workbook.xml.rels", Encoding.UTF8.GetBytes(relationships)),
        };
        parts.AddRange(worksheets.Select(w => (w.path, Encoding.UTF8.GetBytes(w.xml))));
        parts.Add(("xl/styles.xml", Encoding.UTF8.GetBytes(styles)));
        parts.Add(("xl/sharedStrings.xml", Encoding.UTF8.GetBytes(shared)));
        parts.Add(("xl/vbaProject.bin", project));
        using var memory = new MemoryStream();
        using (var archive = new ZipArchive(memory, ZipArchiveMode.Create, true))
            foreach (var (name, body) in parts)
            {
                var entry = archive.CreateEntry(name, CompressionLevel.Optimal);
                entry.LastWriteTime = new DateTimeOffset(1980, 1, 1, 0, 0, 0, TimeSpan.Zero);
                using var stream = entry.Open();
                stream.Write(body);
            }
        return memory.ToArray();
    }

    // --- the VBA project (MS-OVBA) ------------------------------------------------------------------------

    static byte[] WriteProject(List<Module> modules)
    {
        var streams = new List<(string name, Node node)> { ("_VBA_PROJECT", new Node(new byte[] { 0xCC, 0x61, 0xFF, 0xFF, 0, 1, 0 })) };
        long bytes = 0;
        foreach (var module in modules)
        {
            var source = EncodedSource(module);
            bytes += source.Length;
            if (bytes > MaxProject) throw new RefusedVbaException("the VBA project exceeds its size limit");
            streams.Add((module.Name, new Node(Compress(source))));
        }
        streams.Add(("dir", new Node(Compress(DirStream(modules)))));
        var project = CompoundFile.Write(new List<(string, Node)>
        {
            ("PROJECT", new Node(Encoding.ASCII.GetBytes(ProjectStream(modules)))),
            ("PROJECTwm", new Node(NameMap(modules))),
            ("VBA", new Node(streams)),
        });
        if (project == null || project.Length > MaxProject) throw new RefusedVbaException("the VBA project exceeds its size limit");
        return project;
    }

    static byte[] EncodedSource(Module module)
    {
        var normalized = module.Source.Replace("\r\n", "\n").Replace('\r', '\n');
        var text = new StringBuilder($"Attribute VB_Name = \"{module.Name}\"\r\n");
        string baseId = module.Kind switch
        {
            "Workbook" => "00020819-0000-0000-C000-000000000046",
            "Worksheet" => "00020820-0000-0000-C000-000000000046",
            _ => null,
        };
        if (baseId != null)
            text.Append($"Attribute VB_Base = \"0{{{baseId}}}\"\r\nAttribute VB_GlobalNameSpace = False\r\n"
                + "Attribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = True\r\nAttribute VB_Exposed = True\r\n"
                + "Attribute VB_TemplateDerived = False\r\nAttribute VB_Customizable = True\r\n");
        text.Append(normalized.Replace("\n", "\r\n"));
        // The policy admits ASCII only, so Windows-1252 and ASCII agree.
        if (text.ToString().Any(c => c > '~' || c == '\0')) throw new RefusedVbaException("module source is not ASCII");
        return Encoding.ASCII.GetBytes(text.ToString());
    }

    static void Record(List<byte> dir, ushort id, params byte[] body)
    {
        dir.AddRange(BitConverter.GetBytes(id));
        dir.AddRange(BitConverter.GetBytes((uint)body.Length));
        dir.AddRange(body);
    }

    static byte[] Utf16(string text) => Encoding.Unicode.GetBytes(text);

    static byte[] DirStream(List<Module> modules)
    {
        var d = new List<byte>();
        Record(d, 0x01, BitConverter.GetBytes(3u));
        Record(d, 0x4A, BitConverter.GetBytes(6u));
        Record(d, 0x02, BitConverter.GetBytes(0x0409u));
        Record(d, 0x14, BitConverter.GetBytes(0x0409u));
        Record(d, 0x03, BitConverter.GetBytes((ushort)1252));
        Record(d, 0x04, Encoding.ASCII.GetBytes(ProjectName));
        foreach (ushort id in new ushort[] { 0x05, 0x40, 0x06, 0x3D }) Record(d, id);
        Record(d, 0x07, new byte[4]);
        Record(d, 0x08, new byte[4]);
        Record(d, 0x09, BitConverter.GetBytes(1u));
        d.AddRange(BitConverter.GetBytes((ushort)0));
        Record(d, 0x0C);
        Record(d, 0x3C);
        foreach (var (name, libid) in new[]
        {
            ("stdole", @"*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation"),
            ("Office", @"*\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.0#0#C:\Program Files\Common Files\Microsoft Shared\OFFICE16\MSO.DLL#Microsoft Office 16.0 Object Library"),
        })
        {
            Record(d, 0x16, Encoding.ASCII.GetBytes(name));
            Record(d, 0x3E, Utf16(name));
            var registered = BitConverter.GetBytes((uint)libid.Length).Concat(Encoding.ASCII.GetBytes(libid)).Concat(new byte[6]).ToArray();
            Record(d, 0x0D, registered);
        }
        Record(d, 0x0F, BitConverter.GetBytes((ushort)modules.Count));
        Record(d, 0x13, new byte[2]);
        foreach (var module in modules)
        {
            var name = Encoding.ASCII.GetBytes(module.Name);
            var wide = Utf16(module.Name);
            Record(d, 0x19, name);
            Record(d, 0x47, wide);
            Record(d, 0x1A, name);
            Record(d, 0x32, wide);
            Record(d, 0x1C);
            Record(d, 0x48);
            Record(d, 0x31, new byte[4]);
            Record(d, 0x1E, new byte[4]);
            Record(d, 0x2C, new byte[2]);
            Record(d, module.Kind == "Standard" ? (ushort)0x21 : (ushort)0x22);
            Record(d, 0x2B);
        }
        Record(d, 0x10);
        return d.ToArray();
    }

    static byte[] NameMap(List<Module> modules)
    {
        var map = new List<byte>();
        foreach (var module in modules)
        {
            map.AddRange(Encoding.ASCII.GetBytes(module.Name));
            map.Add(0);
            map.AddRange(Utf16(module.Name));
            map.AddRange(new byte[2]);
        }
        map.AddRange(new byte[2]);
        return map.ToArray();
    }

    static string ProjectProperty(byte[] data)
    {
        byte key = 0;
        foreach (var b in Encoding.ASCII.GetBytes(ProjectId)) key = unchecked((byte)(key + b));
        byte previous = key, earlier = 2, unencoded = key;
        var result = new StringBuilder("00").Append(earlier.ToString("X2")).Append(previous.ToString("X2"));
        foreach (var value in BitConverter.GetBytes((uint)data.Length).Concat(data))
        {
            byte encoded = (byte)(value ^ unchecked((byte)(earlier + unencoded)));
            result.Append(encoded.ToString("X2"));
            earlier = previous;
            previous = encoded;
            unencoded = value;
        }
        return result.ToString();
    }

    static string ProjectStream(List<Module> modules)
    {
        var text = new StringBuilder($"ID=\"{ProjectId}\"\r\n");
        foreach (var module in modules)
            text.Append(module.Kind == "Standard" ? $"Module={module.Name}\r\n" : $"Document={module.Name}/&H00000000\r\n");
        text.Append($"Name=\"{ProjectName}\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\n");
        foreach (var (property, data) in new[] { ("CMG", new byte[4]), ("DPB", new byte[] { 0 }), ("GC", new byte[] { 0xFF }) })
            text.Append($"{property}=\"{ProjectProperty(data)}\"\r\n");
        text.Append("\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n");
        return text.ToString();
    }

    /// <summary>An MS-OVBA compressed container (2.4.1.3): copy tokens for repeats, literals otherwise, a raw chunk
    /// only for an incompressible full 4096-byte chunk. An incompressible short final chunk cannot be stored.</summary>
    public static byte[] Compress(byte[] input)
    {
        var output = new List<byte> { 1 };
        for (int start = 0; start < input.Length; start += 4096)
        {
            var chunk = input.AsSpan(start, Math.Min(4096, input.Length - start)).ToArray();
            var packed = CompressChunk(chunk);
            if (packed.Count <= 4096)
            {
                ushort header = (ushort)(0xB000 | (packed.Count - 1));
                output.AddRange(BitConverter.GetBytes(header));
                output.AddRange(packed);
            }
            else if (chunk.Length == 4096)
            {
                output.AddRange(BitConverter.GetBytes((ushort)0x3FFF));
                output.AddRange(chunk);
            }
            else throw new RefusedVbaException("a module does not compress into one chunk; add or remove a comment line");
        }
        return output.ToArray();
    }

    static List<byte> CompressChunk(byte[] chunk)
    {
        const int Candidates = 128;
        var seen = new Dictionary<int, List<int>>();
        void Insert(int at)
        {
            if (at + 3 > chunk.Length) return;
            int key = chunk[at] << 16 | chunk[at + 1] << 8 | chunk[at + 2];
            if (!seen.TryGetValue(key, out var list)) seen[key] = list = new();
            list.Add(at);
        }
        var output = new List<byte>();
        int pos = 0;
        while (pos < chunk.Length)
        {
            int flagsAt = output.Count;
            output.Add(0);
            byte flags = 0;
            for (int bit = 0; bit < 8; bit++)
            {
                if (pos >= chunk.Length) break;
                int bits = Math.Max(32 - System.Numerics.BitOperations.LeadingZeroCount((uint)Math.Max(pos - 1, 0)), 4);
                int longest = (0xFFFF >> bits) + 3;
                int bestDistance = 0, bestLength = 0;
                if (pos + 3 <= chunk.Length
                    && seen.TryGetValue(chunk[pos] << 16 | chunk[pos + 1] << 8 | chunk[pos + 2], out var candidates))
                {
                    for (int c = candidates.Count - 1; c >= Math.Max(0, candidates.Count - Candidates); c--)
                    {
                        int from = candidates[c], length = 0, limit = Math.Min(longest, chunk.Length - pos);
                        while (length < limit && chunk[from + length] == chunk[pos + length]) length++;
                        if (length > bestLength) { bestDistance = pos - from; bestLength = length; }
                    }
                }
                if (bestLength >= 3)
                {
                    ushort token = (ushort)(((bestDistance - 1) << (16 - bits)) | (bestLength - 3));
                    output.AddRange(BitConverter.GetBytes(token));
                    flags |= (byte)(1 << bit);
                    for (int covered = pos; covered < pos + bestLength; covered++) Insert(covered);
                    pos += bestLength;
                }
                else
                {
                    output.Add(chunk[pos]);
                    Insert(pos);
                    pos++;
                }
            }
            output[flagsAt] = flags;
        }
        return output;
    }

    // --- the compound file (MS-CFB, version 3), one FAT and no DIFAT sectors -------------------------------

    sealed class Node
    {
        public readonly byte[] Data;
        public readonly List<(string name, Node node)> Children;
        public Node(byte[] data) { Data = data; }
        public Node(List<(string, Node)> children) { Children = children; }
    }

    static class CompoundFile
    {
        const int Sector = 512;
        const uint End = 0xFFFFFFFE, FatMark = 0xFFFFFFFD, None = 0xFFFFFFFF;

        sealed class Entry
        {
            public string Name;
            public byte Kind;
            public uint Left = None, Right = None, Child = None, Start = End;
            public bool Red;
            public long Size;
            public byte[] Data = System.Array.Empty<byte>();
        }

        static int Compare(string a, string b)
        {
            var x = Encoding.Unicode.GetBytes(a.ToUpperInvariant()).Chunk(2).Select(p => p[0] | p[1] << 8).ToArray();
            var y = Encoding.Unicode.GetBytes(b.ToUpperInvariant()).Chunk(2).Select(p => p[0] | p[1] << 8).ToArray();
            if (x.Length != y.Length) return x.Length.CompareTo(y.Length);
            for (int i = 0; i < x.Length; i++)
                if (x[i] != y[i]) return x[i].CompareTo(y[i]);
            return 0;
        }

        static uint Add(List<Entry> entries, List<(string name, Node node)> children)
        {
            if (children.Count == 0) return None;
            children = children.OrderBy(c => c.name, Comparer<string>.Create(Compare)).ToList();
            var ids = new List<uint>();
            var storages = new List<(uint id, List<(string, Node)> kids)>();
            foreach (var (name, node) in children)
            {
                uint id = (uint)entries.Count;
                ids.Add(id);
                var entry = new Entry { Name = name };
                if (node.Children != null) { storages.Add((id, node.Children)); entry.Kind = 1; }
                else { entry.Kind = 2; entry.Data = node.Data; }
                entries.Add(entry);
            }
            int depthMax = 0;
            while ((1 << depthMax) - 1 < ids.Count) depthMax++;
            bool complete = (1 << depthMax) - 1 == ids.Count;
            uint Build(List<uint> slice, int depth)
            {
                if (slice.Count == 0) return None;
                int mid = slice.Count / 2;
                uint id = slice[mid];
                uint left = Build(slice.GetRange(0, mid), depth + 1);
                uint right = Build(slice.GetRange(mid + 1, slice.Count - mid - 1), depth + 1);
                entries[(int)id].Left = left;
                entries[(int)id].Right = right;
                entries[(int)id].Red = !complete && depth + 1 == depthMax;
                return id;
            }
            uint root = Build(ids, 0);
            foreach (var (id, kids) in storages) entries[(int)id].Child = Add(entries, kids);
            return root;
        }

        static int Ceil(int n, int d) => (n + d - 1) / d;

        public static byte[] Write(List<(string, Node)> root)
        {
            var entries = new List<Entry> { new Entry { Name = "Root Entry", Kind = 5 } };
            entries[0].Child = Add(entries, root);
            var mini = new List<byte>();
            var miniFat = new List<uint>();
            foreach (var entry in entries.Where(e => e.Kind == 2))
            {
                entry.Size = entry.Data.Length;
                if (entry.Data.Length == 0 || entry.Data.Length >= 4096) continue;
                entry.Start = (uint)miniFat.Count;
                int units = Ceil(entry.Data.Length, 64);
                for (int u = 0; u < units; u++) miniFat.Add(u + 1 == units ? End : (uint)miniFat.Count + 1);
                mini.AddRange(entry.Data);
                while (mini.Count < miniFat.Count * 64) mini.Add(0);
            }
            entries[0].Size = mini.Count;
            int directorySectors = Ceil(entries.Count * 128, Sector);
            int miniFatSectors = Ceil(miniFat.Count * 4, Sector);
            int miniSectors = Ceil(mini.Count, Sector);
            var big = entries.Where(e => e.Kind == 2 && e.Data.Length >= 4096).ToList();
            int bigSectors = big.Sum(e => Ceil(e.Data.Length, Sector));
            int dataSectors = directorySectors + miniFatSectors + miniSectors + bigSectors;
            int fatSectors = 1;
            while (Ceil(dataSectors + fatSectors, 128) != fatSectors) fatSectors = Ceil(dataSectors + fatSectors, 128);
            if (fatSectors > 109) throw new RefusedVbaException("the VBA project exceeds its size limit");
            var fat = Enumerable.Repeat(None, fatSectors * 128).ToArray();
            int next = 0;
            for (int i = 0; i < fatSectors; i++) fat[next++] = FatMark;
            uint Chain(int count)
            {
                if (count == 0) return End;
                uint start = (uint)next;
                for (int k = 0; k < count; k++) { fat[next] = k + 1 == count ? End : (uint)next + 1; next++; }
                return start;
            }
            uint directoryStart = Chain(directorySectors), miniFatStart = Chain(miniFatSectors), miniStart = Chain(miniSectors);
            foreach (var entry in big) entry.Start = Chain(Ceil(entry.Data.Length, Sector));
            entries[0].Start = mini.Count == 0 ? End : miniStart;

            var output = new List<byte>(new byte[Sector]);
            void Put(int at, byte[] value) { for (int i = 0; i < value.Length; i++) output[at + i] = value[i]; }
            Put(0, new byte[] { 0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1 });
            Put(0x18, BitConverter.GetBytes((ushort)0x3E));
            Put(0x1A, BitConverter.GetBytes((ushort)3));
            Put(0x1C, BitConverter.GetBytes((ushort)0xFFFE));
            Put(0x1E, BitConverter.GetBytes((ushort)9));
            Put(0x20, BitConverter.GetBytes((ushort)6));
            Put(0x2C, BitConverter.GetBytes((uint)fatSectors));
            Put(0x30, BitConverter.GetBytes(directoryStart));
            Put(0x38, BitConverter.GetBytes(4096u));
            Put(0x3C, BitConverter.GetBytes(miniFatStart));
            Put(0x40, BitConverter.GetBytes((uint)miniFatSectors));
            Put(0x44, BitConverter.GetBytes(End));
            Put(0x48, BitConverter.GetBytes(0u));
            for (int k = 0; k < 109; k++) Put(0x4C + 4 * k, BitConverter.GetBytes(k < fatSectors ? (uint)k : None));
            foreach (var value in fat) output.AddRange(BitConverter.GetBytes(value));
            var directory = new List<byte>();
            foreach (var entry in entries)
            {
                var e = new byte[128];
                var units = entry.Name.ToCharArray();
                for (int k = 0; k < Math.Min(31, units.Length); k++) BitConverter.GetBytes(units[k]).CopyTo(e, 2 * k);
                BitConverter.GetBytes((ushort)((Math.Min(31, units.Length) + 1) * 2)).CopyTo(e, 0x40);
                e[0x42] = entry.Kind;
                e[0x43] = (byte)(entry.Red ? 0 : 1);
                BitConverter.GetBytes(entry.Left).CopyTo(e, 0x44);
                BitConverter.GetBytes(entry.Right).CopyTo(e, 0x48);
                BitConverter.GetBytes(entry.Child).CopyTo(e, 0x4C);
                BitConverter.GetBytes(entry.Start).CopyTo(e, 0x74);
                BitConverter.GetBytes(entry.Size).CopyTo(e, 0x78);
                directory.AddRange(e);
            }
            while (directory.Count < directorySectors * Sector) directory.Add(0);
            for (int k = entries.Count; k < directorySectors * 4; k++)
                for (int b = 0x44; b < 0x50; b++) directory[k * 128 + b] = 0xFF;
            output.AddRange(directory);
            foreach (var value in miniFat) output.AddRange(BitConverter.GetBytes(value));
            int through = Sector * (1 + fatSectors + directorySectors + miniFatSectors);
            while (output.Count < through) output.Add(0xFF);
            output.AddRange(mini);
            through += miniSectors * Sector;
            while (output.Count < through) output.Add(0);
            foreach (var entry in big)
            {
                output.AddRange(entry.Data);
                while (output.Count % Sector != 0) output.Add(0);
            }
            return output.ToArray();
        }
    }
}
