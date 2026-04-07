// Polyfills.cs
// Provides types that exist in .NET 5+ but are absent from netstandard2.1.
// The C# 9 compiler looks for these by name; providing them here satisfies
// the lookup without requiring a TFM bump.

namespace System.Runtime.CompilerServices
{
    /// <summary>
    /// Polyfill that enables C# 9 `init`-only setters and positional records
    /// on netstandard2.1 / .NET Framework targets.
    /// The compiler resolves this type by name — the class body must be empty.
    /// </summary>
    internal static class IsExternalInit { }
}
