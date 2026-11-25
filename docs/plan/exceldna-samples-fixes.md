# Excel-DNA/samples: fix proposals

Proposed changes to [`Excel-DNA/samples`](https://github.com/Excel-DNA/samples), one unified diff per sample, against commit `ba5732c7d` (the commit the problems list inspected). Nothing was pushed or opened as a pull request there; the owner decides. The diffs were made from a scratch copy of a read-only clone and check with `git apply --check` against that commit.

Evidence comes from `docs/plan/exceldna-samples-problems.md`, `docs/plan/exceldna-samples.tsv` (Windows builds, dc-z05nj.2) and the ww17 and ww18 private-Excel runs (dc-z05nj.3). **None of these diffs has been built or run**: this lane runs on Linux. Each section says what the diff rests on and what must be run on Windows. Entries marked *unverified* rest on an error message and documentation only.

Apply with `git apply <file>` from the root of a samples checkout (each block below is one patch).

## AddInReloader

**Evidence.** Windows ww18: all four packed builds loaded, but after the watched file changed the control `AddThem(2,3)` still returned `#NAME?` (no reload). `AddInReloaderConfiguration.xml` is not copied to the output folder, so `AutoOpen` throws in `File.OpenRead`, the exception is logged, and `_watcher` stays null: nothing watches anything. `AutoClose` then throws a `NullReferenceException`. Static findings (problems list): the stream is never closed, and the watcher dictionary is never filled.

**Still to run.** Windows: rebuild, run the add-in with a config naming a built control add-in, change the watched DLL, check `AddThem` recalculates (not yet run).

```diff
diff --git a/AddInReloader/AddIn.cs b/AddInReloader/AddIn.cs
index a992805..d1c83c7 100644
--- a/AddInReloader/AddIn.cs
+++ b/AddInReloader/AddIn.cs
@@ -23,7 +23,11 @@ public void AutoOpen()
             {
                 // Load config
                 XmlSerializer configLoader = new XmlSerializer(typeof(AddInReloaderConfiguration));
-                AddInReloaderConfiguration config = (AddInReloaderConfiguration)configLoader.Deserialize(File.OpenRead(configPath));
+                AddInReloaderConfiguration config;
+                using (var stream = File.OpenRead(configPath))
+                {
+                    config = (AddInReloaderConfiguration)configLoader.Deserialize(stream);
+                }
                 _watcher = new AddInWatcher(config);
             }
             catch (Exception ex)
@@ -34,7 +38,7 @@ public void AutoOpen()
 
         public void AutoClose()
         {
-            _watcher.Dispose();
+            _watcher?.Dispose();
         }
     }
 }
diff --git a/AddInReloader/AddInReloader.csproj b/AddInReloader/AddInReloader.csproj
index 188cae7..46471c1 100644
--- a/AddInReloader/AddInReloader.csproj
+++ b/AddInReloader/AddInReloader.csproj
@@ -6,6 +6,9 @@
 
 	<ItemGroup>
 		<PackageReference Include="ExcelDna.Addin" Version="*-*" />
+		<None Update="AddInReloaderConfiguration.xml">
+			<CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
+		</None>
 	</ItemGroup>
 
 </Project>
\ No newline at end of file
diff --git a/AddInReloader/AddInWatcher.cs b/AddInReloader/AddInWatcher.cs
index 839932d..b271180 100644
--- a/AddInReloader/AddInWatcher.cs
+++ b/AddInReloader/AddInWatcher.cs
@@ -27,6 +27,7 @@ public AddInWatcher(AddInReloaderConfiguration config)
                     if (!_watchedDirectories.TryGetValue(directory, out wd))
                     {
                         wd = new WatchedDirectory(directory, InvalidateAddIn);
+                        _watchedDirectories[directory] = wd;
                     }
                     wd.WatchAddIn(addIn);
                 }
```

## ArrayMapCs

**Evidence.** Windows build (dc-z05nj.2): `net472` fails with CS0138 and CS0234; `net6.0-windows` fails with CS0138. The file imports `ExcelDna.Integration.XlCall` (a class) as a namespace and `Microsoft.VisualBasic.FileIO` (not referenced on net472). The code calls `XlCall.Excel(...)` fully qualified and uses nothing from `FileIO`, so both usings go. The VB twin `ArrayMap` builds.

**Still to run.** Windows: `dotnet build ArrayMapCs` for both frameworks (not yet run).

```diff
diff --git a/ArrayMapCs/Functions.cs b/ArrayMapCs/Functions.cs
index b22025f..4543172 100644
--- a/ArrayMapCs/Functions.cs
+++ b/ArrayMapCs/Functions.cs
@@ -1,8 +1,6 @@
 using System;
 using System.Collections.Generic;
 using ExcelDna.Integration;
-using ExcelDna.Integration.XlCall;
-using Microsoft.VisualBasic.FileIO;
 
 public static class Functions
 {
```

## ComServer and DnaComServer

**Evidence.** Windows build: `net6.0-windows` fails in both with `An error occurred trying to start process 'dscom' ... The system cannot find the file specified` (`net472` builds). The projects point `ExcelAddInTlbExp` at the checked-in `TlbExp.exe`, which cannot read a .NET 6 assembly, so the build falls back to a `dscom` tool that is not installed. `DnaComServer/Readme.md:96` already tells users to add `dSPACE.Runtime.InteropServices.BuildTasks`. The diff keeps `TlbExp.exe` for `net472` only and adds that package for the other framework.

**Still to run.** Unverified: written from the error text and the README; needs a Windows `dotnet build` of both projects for `net6.0-windows` and a check that the `.tlb` appears and VBA early binding works. The package version `1.2.0` is the README's.

```diff
diff --git a/ComServer/ComServer.csproj b/ComServer/ComServer.csproj
index fc1ad14..89cfa00 100644
--- a/ComServer/ComServer.csproj
+++ b/ComServer/ComServer.csproj
@@ -5,11 +5,21 @@
 	</PropertyGroup>
 
   	<PropertyGroup>
+	    <ExcelAddInComServer>true</ExcelAddInComServer>
+	</PropertyGroup>
+
+	<!-- .NET Framework: the Windows SDK's tlbexp builds the type library. -->
+	<PropertyGroup Condition="'$(TargetFramework)' == 'net472'">
 	    <ExcelAddInTlbExp>$(MSBuildProjectDirectory)\TlbExp.exe</ExcelAddInTlbExp>
 	    <ExcelAddInTlbCreate>true</ExcelAddInTlbCreate>
-	    <ExcelAddInComServer>true</ExcelAddInComServer>
 	</PropertyGroup>
 
+	<!-- .NET 6+: tlbexp cannot read the assembly. The dSPACE build task writes the type library
+	     in-process, so the build does not look for a `dscom` tool on PATH. -->
+	<ItemGroup Condition="'$(TargetFramework)' != 'net472'">
+	    <PackageReference Include="dSPACE.Runtime.InteropServices.BuildTasks" Version="1.2.0" PrivateAssets="all" />
+	</ItemGroup>
+
 	<ItemGroup>
 		<PackageReference Include="ExcelDna.Addin" Version="*-*" />
 	</ItemGroup>
diff --git a/DnaComServer/DnaComServer.csproj b/DnaComServer/DnaComServer.csproj
index 58ae388..3172412 100644
--- a/DnaComServer/DnaComServer.csproj
+++ b/DnaComServer/DnaComServer.csproj
@@ -6,11 +6,21 @@
 	</PropertyGroup>
 
 	<PropertyGroup>
+	    <ExcelAddInComServer>true</ExcelAddInComServer>
+	</PropertyGroup>
+
+	<!-- .NET Framework: the Windows SDK's tlbexp builds the type library. -->
+	<PropertyGroup Condition="'$(TargetFramework)' == 'net472'">
 	    <ExcelAddInTlbExp>$(MSBuildProjectDirectory)\TlbExp.exe</ExcelAddInTlbExp>
 	    <ExcelAddInTlbCreate>true</ExcelAddInTlbCreate>
-	    <ExcelAddInComServer>true</ExcelAddInComServer>
 	</PropertyGroup>
 
+	<!-- .NET 6+: tlbexp cannot read the assembly. The dSPACE build task writes the type library
+	     in-process, so the build does not look for a `dscom` tool on PATH. -->
+	<ItemGroup Condition="'$(TargetFramework)' != 'net472'">
+	    <PackageReference Include="dSPACE.Runtime.InteropServices.BuildTasks" Version="1.2.0" PrivateAssets="all" />
+	</ItemGroup>
+
 	<ItemGroup>
 		<PackageReference Include="ExcelDna.Addin" Version="*-*" />
 	</ItemGroup>
```

## ComServerVB

**Evidence.** Static (problems list): `Class1.vb:21` calls `ComServer.DllRegisterServer()` but the project never sets `ExcelAddInComServer`, unlike the C# `ComServer`.

**Still to run.** Windows: build and call the COM class from VBA (not yet run).

```diff
diff --git a/ComServerVB/ComAddin.vbproj b/ComServerVB/ComAddin.vbproj
index 41a707f..8576a8b 100644
--- a/ComServerVB/ComAddin.vbproj
+++ b/ComServerVB/ComAddin.vbproj
@@ -2,6 +2,7 @@
 
 	<PropertyGroup>
 		<TargetFrameworks>net472;net6.0-windows</TargetFrameworks>
+		<ExcelAddInComServer>true</ExcelAddInComServer>
 	</PropertyGroup>
 
 	<ItemGroup>
```

## DynamicCompiledVB

**Evidence.** Windows build: all four builds (both frameworks, both package sets) fail with BC36924: `Expression(Of Func(Of Object, Range))` crosses an assembly boundary with an embedded interop type. `ExcelDna.Interop` embeds the Excel interop types by default; `ExcelDna.Registration.VisualBasic` exposes `Range` in that signature, so the interop assembly has to be linked, not embedded.

**Still to run.** Unverified: the metadata update is the usual remedy for BC36924 but was not built here. Windows `dotnet build` needed. The separate runtime defect (`VBCodeProvider` throws on .NET 6+) is not addressed.

```diff
diff --git a/DynamicCompiledVB/DynamicCompiledVB.vbproj b/DynamicCompiledVB/DynamicCompiledVB.vbproj
index 7eeec68..8a6eb19 100644
--- a/DynamicCompiledVB/DynamicCompiledVB.vbproj
+++ b/DynamicCompiledVB/DynamicCompiledVB.vbproj
@@ -15,6 +15,16 @@
 		<PackageReference Include="System.Configuration.ConfigurationManager" Version="7.0.0" />
 	</ItemGroup>
 
+	<!-- ExcelDna.Registration.VisualBasic exposes Expression(Of Func(Of Object, Range)); an embedded
+	     Excel interop Range cannot cross that assembly boundary (BC36924), so link the interop assembly. -->
+	<Target Name="LinkExcelInteropAssembly" AfterTargets="ResolveAssemblyReferences">
+		<ItemGroup>
+			<ReferencePath Condition="'%(Filename)' == 'Microsoft.Office.Interop.Excel'">
+				<EmbedInteropTypes>false</EmbedInteropTypes>
+			</ReferencePath>
+		</ItemGroup>
+	</Target>
+
 	<ItemGroup>
 		<Folder Include="Properties\" />
 	</ItemGroup>
```

## ObjectHandles and AsyncAwaitMacro (typos)

**Evidence.** Static (problems list): `Hello form ObjectHandles!`, a stray `; ;`, and two unclosed string literals in the AsyncAwaitMacro README snippet.

**Still to run.** None needed beyond a build.

```diff
diff --git a/AsyncAwaitMacro/README.md b/AsyncAwaitMacro/README.md
index 6ae169c..baaf74a 100644
--- a/AsyncAwaitMacro/README.md
+++ b/AsyncAwaitMacro/README.md
@@ -30,9 +30,9 @@ static async Task SlowWork()
     // All the code here, before and after the awaits, will run on the main thread in a macro context
     // (where C API calls and the COM object model is safe to access).
     await SomeWorkAsync();
-    Application.Range["A1"].Value = "abc;
+    Application.Range["A1"].Value = "abc";
     await OtherWorkAsync();
-    Application.Range["A2"].Value = "xyz;
+    Application.Range["A2"].Value = "xyz";
 }
 
 ```
diff --git a/ObjectHandles/ExcelFunctions.cs b/ObjectHandles/ExcelFunctions.cs
index 0c0b7b5..239ab16 100644
--- a/ObjectHandles/ExcelFunctions.cs
+++ b/ObjectHandles/ExcelFunctions.cs
@@ -13,7 +13,7 @@ public static class ExcelFunctions
 
         public static object TestHandleSample()
         {
-            return "Hello form ObjectHandles!";
+            return "Hello from ObjectHandles!";
         }
 
         public static object CreateDataObject1(string code)
diff --git a/ObjectHandles/ObjectHandler.cs b/ObjectHandles/ObjectHandler.cs
index a8475b8..a16fdbc 100644
--- a/ObjectHandles/ObjectHandler.cs
+++ b/ObjectHandles/ObjectHandler.cs
@@ -50,7 +50,7 @@ public IDisposable Subscribe(IExcelObserver observer)
         internal void Update(IHasRowVersion target)
         {
             Target = target;
-            Handle = string.Format("{0}:{1}", ObjectType, HandleIndex++); ;
+            Handle = string.Format("{0}:{1}", ObjectType, HandleIndex++);
             LastUpdate = DateTime.Now;          // Might be used to decide when or how often to refresh
             if (Observer != null)
                 Observer.OnNext(Handle);        // Triggers the update sending the new handle to Excel
```

## Registration (Registration.Sample.VisualBasic)

**Evidence.** Static (problems list): `SignAssembly` is on with no key file; the C# and F# siblings do not set it. Windows build: the project built, so this is a hygiene fix, not a build fix.

**Still to run.** Windows build of the project (not yet run).

```diff
diff --git a/Registration/Registration.Sample.VisualBasic/Registration.Sample.VisualBasic.vbproj b/Registration/Registration.Sample.VisualBasic/Registration.Sample.VisualBasic.vbproj
index 409280e..06cd0b4 100644
--- a/Registration/Registration.Sample.VisualBasic/Registration.Sample.VisualBasic.vbproj
+++ b/Registration/Registration.Sample.VisualBasic/Registration.Sample.VisualBasic.vbproj
@@ -6,7 +6,6 @@
 		<RootNamespace>Registration.Sample.VisualBasic</RootNamespace>
 		<OptionCompare>Text</OptionCompare>
 
-		<SignAssembly>true</SignAssembly>
 	</PropertyGroup>
 
 	<ItemGroup>
```

## RtdPerformance

**Evidence.** Windows ww18: all eight packed artifacts loaded but `rtdHello()` returned `#NAME?`. Both projects sit in `v032/` and `v110/` and compile only files under their own folder; the add-in's code is in `../Source/`, so the DLLs are empty and export nothing. `Source/AddIn.cs:16` also calls `ComServer.DllRegisterServer()` without `ExcelAddInComServer`. Both `AssemblyInfo.cs` files carry the same GUID. Not covered here: `Thread.Abort()` in `Source/DataService.cs:43` throws on `net6.0-windows` (needs a cooperative-cancel rewrite), and `*-*` makes `v032` and `v110` the same version.

**Still to run.** Windows: rebuild, register, check `rtdHello()` and the example workbooks (not yet run).

```diff
diff --git a/RtdPerformance/v032/Properties/AssemblyInfo.cs b/RtdPerformance/v032/Properties/AssemblyInfo.cs
index 061fa96..04c8e66 100644
--- a/RtdPerformance/v032/Properties/AssemblyInfo.cs
+++ b/RtdPerformance/v032/Properties/AssemblyInfo.cs
@@ -20,7 +20,7 @@
 [assembly: ComVisible(false)]
 
 // The following GUID is for the ID of the typelib if this project is exposed to COM
-[assembly: Guid("e5ed525a-5168-4e0d-a7f3-89d56e833237")]
+[assembly: Guid("5b6c0a1e-3f2d-4c8a-9e57-0d4f1a7b2c93")]
 
 // Version information for an assembly consists of the following four values:
 //
diff --git a/RtdPerformance/v032/RtdPerformance-v032.csproj b/RtdPerformance/v032/RtdPerformance-v032.csproj
index 71d8976..342fa61 100644
--- a/RtdPerformance/v032/RtdPerformance-v032.csproj
+++ b/RtdPerformance/v032/RtdPerformance-v032.csproj
@@ -2,8 +2,13 @@
 
 	<PropertyGroup>
 		<TargetFrameworks>net472;net6.0-windows</TargetFrameworks>
+		<ExcelAddInComServer>true</ExcelAddInComServer>
 	</PropertyGroup>
 
+	<ItemGroup>
+		<Compile Include="..\Source\*.cs" />
+	</ItemGroup>
+
 	<ItemGroup>
 		<PackageReference Include="ExcelDna.Addin" Version="*-*" />
 	</ItemGroup>
diff --git a/RtdPerformance/v110/RtdPerformance-v110.csproj b/RtdPerformance/v110/RtdPerformance-v110.csproj
index 71d8976..342fa61 100644
--- a/RtdPerformance/v110/RtdPerformance-v110.csproj
+++ b/RtdPerformance/v110/RtdPerformance-v110.csproj
@@ -2,8 +2,13 @@
 
 	<PropertyGroup>
 		<TargetFrameworks>net472;net6.0-windows</TargetFrameworks>
+		<ExcelAddInComServer>true</ExcelAddInComServer>
 	</PropertyGroup>
 
+	<ItemGroup>
+		<Compile Include="..\Source\*.cs" />
+	</ItemGroup>
+
 	<ItemGroup>
 		<PackageReference Include="ExcelDna.Addin" Version="*-*" />
 	</ItemGroup>
```

## Archive/Async/AsyncFunctions

**Evidence.** Windows build: `net45` fails with CS0234/CS0246 for `System.Reactive` and `ExcelDna.Integration.Rtd`. The project references `..\..\..\ExcelDna.Integration.dll`, a file that is not in the repository, and its `packages.config` packages are only restored by `nuget restore` or `msbuild -restore`, not `dotnet build`.

**Still to run.** Unverified: package version and `lib` folder (`1.1.0`, `net40`) are a guess from the 2016-era Reactive 3.0.0 pin; check on Windows with `msbuild -restore`. If it cannot be made to build, mark the folder as not built in `Archive/README`.

```diff
diff --git a/Archive/Async/AsyncFunctions/AsyncFunctions.csproj b/Archive/Async/AsyncFunctions/AsyncFunctions.csproj
index 0929b93..fefa7ca 100644
--- a/Archive/Async/AsyncFunctions/AsyncFunctions.csproj
+++ b/Archive/Async/AsyncFunctions/AsyncFunctions.csproj
@@ -37,7 +37,7 @@
   </PropertyGroup>
   <ItemGroup>
     <Reference Include="ExcelDna.Integration">
-      <HintPath>..\..\..\ExcelDna.Integration.dll</HintPath>
+      <HintPath>packages\ExcelDna.Integration.1.1.0\lib\net40\ExcelDna.Integration.dll</HintPath>
       <Private>False</Private>
     </Reference>
     <Reference Include="System" />
diff --git a/Archive/Async/AsyncFunctions/packages.config b/Archive/Async/AsyncFunctions/packages.config
index dfd9836..cc36257 100644
--- a/Archive/Async/AsyncFunctions/packages.config
+++ b/Archive/Async/AsyncFunctions/packages.config
@@ -1,5 +1,6 @@
 ﻿<?xml version="1.0" encoding="utf-8"?>
 <packages>
+  <package id="ExcelDna.Integration" version="1.1.0" targetFramework="net45" />
   <package id="System.Reactive" version="3.0.0" targetFramework="net45" />
   <package id="System.Reactive.Core" version="3.0.0" targetFramework="net45" />
   <package id="System.Reactive.Interfaces" version="3.0.0" targetFramework="net45" />
```

## Problems without a diff

These failed in Excel on Windows, but the evidence does not show the cause, so no diff is offered.

- **Registration.Sample (`net462`), UsingLog4Net (`net472`), Logging (`net472`), UsingSQLite (`net472`).** The packed builds returned `#NAME?` (Registration.Sample, UsingLog4Net, Logging) or `#VALUE!` (UsingSQLite); the unpacked controls and the packed `net6.0-windows` builds worked. All four pull in a dependency beside the add-in assembly (a project reference, log4net, NLog or the SQLite native interop DLL). The next step is to read the packed XLL's resource list for the missing assembly or native DLL; that needs a Windows session. Owner: dc-z05nj.3.
- **UsingDAO and UsingRDotNet.** `DBEngineVersion()`, `TestRDotNet()` and `MyRnorm(1)` failed in all four builds each. They need the Access database engine and an R installation, which the test machine may lack; a README prerequisite note is the only change the evidence supports. Owner: dc-z05nj.3.
- **Static problems not in this file.** The thread-safety races (`RtdArrayTest`, `RtdClocks`, `LosslessObservable`), the `SettingsProvider` stubs, the `Misc` snippets and the README mismatches in `problems.md` are listed there with file and line. They need design choices (what to lock, what to implement), so each is an owner decision, not a smallest diff.
