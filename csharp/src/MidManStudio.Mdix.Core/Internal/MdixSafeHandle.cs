using System;
using System.Runtime.InteropServices;
using MidManStudio.DixScript.Native;

namespace MidManStudio.Mdix.Core.Internal
{
    /// <summary>
    /// CLR-managed wrapper around the native mdix void* handle.
    /// The CLR guarantees <see cref="ReleaseHandle"/> is called exactly once
    /// even under exceptional or resource-constrained conditions.
    /// The finalizer is provided by <see cref="SafeHandle"/> itself — no
    /// finalizer is needed on <see cref="MdixDatabase"/>.
    /// </summary>
    internal sealed unsafe class MdixSafeHandle : SafeHandle
    {
        /// <summary>Creates an invalid (empty) handle — required by P/Invoke infrastructure.</summary>
        public MdixSafeHandle() : base(IntPtr.Zero, ownsHandle: true) { }

        /// <summary>Wraps an existing raw native handle pointer.</summary>
        internal MdixSafeHandle(void* rawHandle) : base(IntPtr.Zero, ownsHandle: true)
        {
            SetHandle((IntPtr)rawHandle);
        }

        /// <inheritdoc/>
        public override bool IsInvalid => handle == IntPtr.Zero;

        /// <summary>
        /// Called by the CLR to release the native handle.
        /// Never call this directly — let <see cref="SafeHandle.Dispose"/> or
        /// the finalizer invoke it.
        /// </summary>
        protected override bool ReleaseHandle()
        {
            MdixNative.mdix_free((void*)handle);
            return true;
        }
    }
}
