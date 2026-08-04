/*

    --> mdix-lsp/src/features/hover.rs:1117:4
     |
1117 | fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
     |    ^^^^^^^^^^^^^^^^^

warning: function `hover_table_path_rich` is never used
    --> mdix-lsp/src/features/hover.rs:1179:4
     |
1179 | fn hover_table_path_rich(doc: &Document, path: &str) -> Option<String> {
     |    ^^^^^^^^^^^^^^^^^^^^^

warning: `mdix-lsp` (lib) generated 16 warnings (run `cargo fix --lib -p mdix-lsp` to apply 6 suggestions)
    Finished `release` profile [optimized] target(s) in 51.92s
==> Bundling mdix-lsp binary
Copied: /Users/midman/Desktop/DixScript-Rust/target/release/mdix-lsp
    to: /Users/midman/Desktop/DixScript-Rust/mdix-vsmac/scripts/../bin/darwin-x64/mdix-lsp
==> Building MdixAddin (Release)
objc[19071]: Class AMSupportURLConnectionDelegate is implemented in both /System/Library/PrivateFrameworks/OSPersonalization.framework/Versions/A/OSPersonalization (0x7fff92942510) and /System/Library/PrivateFrameworks/MobileDevice.framework/Versions/A/MobileDevice (0x105d5b630). One of the two will be used. Which one is undefined.
objc[19071]: Class AMSupportURLSession is implemented in both /System/Library/PrivateFrameworks/OSPersonalization.framework/Versions/A/OSPersonalization (0x7fff92942560) and /System/Library/PrivateFrameworks/MobileDevice.framework/Versions/A/MobileDevice (0x105d5b680). One of the two will be used. Which one is undefined.
2026-08-04 21:28:07.180 vstool[19071:243615] Microsoft.macOS: Invalid IDE Port: -1
Visual Studio Build Tool
FATAL ERROR [2026-08-04 21:28:48Z]: System.AggregateException: One or more errors occurred. (Exception has been thrown by the target of an invocation.)
 ---> System.Reflection.TargetInvocationException: Exception has been thrown by the target of an invocation.
 ---> System.AggregateException: One or more errors occurred. (Could not find file '/Users/midman/Library/Application Support/VisualStudio/17.0/LocalInstall/Addins/com.midmanstudio.MdixLanguageSupport.1.0/MdixLanguageSupport.dll'.)
 ---> System.IO.FileNotFoundException: Could not find file '/Users/midman/Library/Application Support/VisualStudio/17.0/LocalInstall/Addins/com.midmanstudio.MdixLanguageSupport.1.0/MdixLanguageSupport.dll'.
File name: '/Users/midman/Library/Application Support/VisualStudio/17.0/LocalInstall/Addins/com.midmanstudio.MdixLanguageSupport.1.0/MdixLanguageSupport.dll'
   at Interop.ThrowExceptionForIoErrno(ErrorInfo errorInfo, String path, Boolean isDirError)
   at Microsoft.Win32.SafeHandles.SafeFileHandle.Open(String fullPath, FileMode mode, FileAccess access, FileShare share, FileOptions options, Int64 preallocationSize, UnixFileMode openPermissions, Int64& fileLength, UnixFileMode& filePermissions, Func`4 createOpenException)
   at System.IO.Strategies.OSFileStreamStrategy..ctor(String path, FileMode mode, FileAccess access, FileShare share, FileOptions options, Int64 preallocationSize, Nullable`1 unixCreateMode)
   at System.Reflection.Metadata.MetadataReader.GetAssemblyName(String assemblyFile)
   at MonoDevelop.Core.RuntimeAssemblyResolver.RegisterAssembly(String assemblyFile) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Core/Runtime.cs:line 763
   at MonoDevelop.Ide.Composition.CompositionManager.InitializeInstanceAsync(ITimeTracker`1 timer, MefAssemblyList mefAssemblies) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Ide/MonoDevelop.Ide.Composition/CompositionManager.cs:line 166
   at MonoDevelop.Ide.Composition.CompositionManager.OnInitialize(ServiceProvider serviceProvider) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Ide/MonoDevelop.Ide.Composition/CompositionManager.cs:line 94
   at MonoDevelop.Core.BasicServiceProvider.Initialize(ServiceRegistration serviceRegistration, Type serviceType) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Core/BasicServiceProvider.cs:line 155
   at MonoDevelop.Core.BasicServiceProvider.GetService[T]() in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Core/BasicServiceProvider.cs:line 92
   --- End of inner exception stack trace ---
   at System.Threading.Tasks.Task.ThrowIfExceptional(Boolean includeTaskCanceledExceptions)
   at System.Threading.Tasks.Task.Wait(Int32 millisecondsTimeout, CancellationToken cancellationToken)
   at System.Threading.Tasks.Task.Wait(CancellationToken cancellationToken)
   at MonoDevelop.Ide.TaskUtil.WaitAndGetResult[T](Task`1 task, CancellationToken cancellationToken) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Ide/MonoDevelop.Ide/TaskUtil.cs:line 54
   at MonoDevelop.Ide.Composition.CompositionManager.get_Instance() in /Users/runner/work/1/s/main/src/core/MonoDevelop.Ide/MonoDevelop.Ide.Composition/CompositionManager.cs:line 73
   at Microsoft.VisualStudio.Mac.RazorAddin.RazorProjectExtension..ctor() in /_/src/Razor/src/Microsoft.VisualStudio.Mac.RazorAddin/RazorProjectExtension.cs:line 22
   at System.RuntimeType.CreateInstanceDefaultCtor(Boolean publicOnly, Boolean wrapExceptions)
   --- End of inner exception stack trace ---
   at System.RuntimeType.CreateInstanceDefaultCtor(Boolean publicOnly, Boolean wrapExceptions)
   at Mono.Addins.TypeExtensionNode.CreateInstance() in /Users/runner/work/1/s/Mono.Addins/Mono.Addins/TypeExtensionNode.cs:line 93
   at Mono.Addins.InstanceExtensionNode.CreateInstance(Type expectedType) in /Users/runner/work/1/s/Mono.Addins/Mono.Addins/InstanceExtensionNode.cs:line 93
   at MonoDevelop.Projects.Extensions.ProjectModelExtensionNode.CreateExtension() in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects.Extensions/ProjectModelExtensionNode.cs:line 41
   at MonoDevelop.Projects.WorkspaceObject.InitializeExtensionChain() in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/WorkspaceObject.cs:line 478
   at MonoDevelop.Projects.WorkspaceObject.EnsureInitialized() in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/WorkspaceObject.cs:line 88
   at MonoDevelop.Projects.MSBuild.MSBuildProjectService.LoadItem(ProgressMonitor monitor, String fileName, MSBuildFileFormat expectedFormat, String typeGuid, String itemGuid, SolutionLoadContext ctx) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects.MSBuild/MSBuildProjectService.cs:line 460
   at MonoDevelop.Projects.SdkProjectReader.LoadSolutionItem(ProgressMonitor monitor, SolutionLoadContext ctx, String fileName, MSBuildFileFormat expectedFormat, String typeGuid, String itemGuid) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/SdkProjectReader.cs:line 113
   at MonoDevelop.Projects.ProjectService.ReadSolutionItem(ProgressMonitor monitor, String file, MSBuildFileFormat format, String typeGuid, StringitemGuid, SolutionLoadContext ctx) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/ProjectService.cs:line 160
   at MonoDevelop.Projects.ProjectService.ReadSolutionItem(ProgressMonitor monitor, String file) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/ProjectService.cs:line 140
   at MonoDevelop.Projects.BuildTool.Run(String[] arguments) in /Users/runner/work/1/s/main/src/core/MonoDevelop.Core/MonoDevelop.Projects/BuildTool.cs:line 121
   --- End of inner exception stack trace ---
   at System.Threading.Tasks.Task`1.GetResultCore(Boolean waitCompletionNotification)
   at VsTool.Main(String[] args) in /Users/runner/work/1/s/main/src/tools/vstool/src/vstool.cs:line 168

 */