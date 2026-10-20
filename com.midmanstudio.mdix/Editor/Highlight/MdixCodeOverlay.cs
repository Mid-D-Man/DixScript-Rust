using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor.Highlight
{
    /// <summary>
    /// Syntax colouring for a multiline UI Toolkit <see cref="TextField"/>.
    ///
    /// Why an overlay: a UI Toolkit TextField edits one plain string and can't
    /// colour ranges of it. Rich text can't simply be switched on in the field
    /// either — Unity forces enableRichText off on the editable text element
    /// (TextInputBaseField.TextInputBase), because the caret/selection indices
    /// would then count markup characters. So instead this:
    ///   1. finds the field's real inner TextElement and makes its glyphs
    ///      transparent (the caret and selection highlight are painted
    ///      separately — TextElementSelection — so they stay visible);
    ///   2. inserts a second TextElement directly beneath it, in the same
    ///      parent, whose rich text is the same characters with colour tags;
    ///   3. keeps the two on exactly the same box, padding, font and scroll.
    ///
    /// All editing, clipboard, IME and selection stay 100% native; if the
    /// overlay can't be attached (Unity changes the field's structure) the
    /// field simply keeps its normal single-colour text.
    ///
    /// Structure and behaviour verified against the 2022.3 UIElements sources
    /// (TextInputFieldBase.cs, TextElementSelection.cs, TextElement.cs); the
    /// rich-text invariant that makes alignment exact — visible characters ==
    /// source text — is unit-tested in MdixRichText.
    /// </summary>
    internal sealed class MdixCodeOverlay
    {
        // TextInputBaseField<T>.TextInputBase.innerTextElementUssClassName is
        // protected-internal, so rebuild the string: TextElement.ussClassName
        // + "--inner-input-field-component".
        private static readonly string InnerClassName =
            TextElement.ussClassName + "--inner-input-field-component";

        private readonly TextField _field;

        private TextElement? _inner;
        private TextElement? _overlay;
        private IVisualElementScheduledItem? _poll;
        private Func<string, string>? _buildRich;

        private string? _lastPlain;
        private Vector3 _lastTranslation = new Vector3(float.NaN, 0, 0);

        public MdixCodeOverlay(TextField field)
        {
            _field = field;
        }

        public bool IsAttached => _overlay != null;

        /// <summary>The field's real text element (null until attached).</summary>
        public TextElement? InnerTextElement => _inner;

        /// <summary>Human-readable reason the last <see cref="Attach"/> failed (for the log).</summary>
        public string FailureReason { get; private set; } = string.Empty;

        // ── Attach / detach ───────────────────────────────────────────────────

        /// <param name="buildRich">Maps the field's plain text to the rich-text string to draw.</param>
        public bool Attach(Func<string, string> buildRich)
        {
            _buildRich = buildRich;
            if (_overlay != null) return true;

            var inner = FindInnerTextElement();
            if (inner == null || inner.parent == null)
            {
                FailureReason = "the TextField's inner text element wasn't found";
                return false;
            }

            var overlay = new TextElement
            {
                pickingMode              = PickingMode.Ignore,
                focusable                = false,
                enableRichText           = true,
                parseEscapeSequences     = false, // the input element does the same: "\n" typed in source stays literal
                displayTooltipWhenElided = false,
            };

            // Same USS classes => same stylesheet-driven font, size, colour inheritance etc.
            foreach (var cls in inner.GetClasses())
                overlay.AddToClassList(cls);

            overlay.style.position    = Position.Absolute;
            overlay.style.marginLeft  = 0;
            overlay.style.marginTop   = 0;
            overlay.style.marginRight = 0;
            overlay.style.marginBottom = 0;
            overlay.style.color       = ColorFromHex(MdixPalette.BaseText);

            var parent = inner.parent;
            parent.Insert(parent.IndexOf(inner), overlay); // underneath: later siblings paint on top

            _inner   = inner;
            _overlay = overlay;

            // Hide only the glyphs. Caret + selection are drawn independently of the text colour.
            inner.style.color = new Color(0f, 0f, 0f, 0f);

            inner.RegisterCallback<GeometryChangedEvent>(OnInnerGeometryChanged);
            SyncBox();

            _lastPlain = null;
            Rebuild(_field.value);

            // Scrolling in the plain (non scroll-view) multiline layout is applied by
            // Unity as a translation on the text element with no event, so follow it
            // by polling. In scroll-view layout the overlay shares the scrolled
            // container and this just never finds a difference. The poll also
            // re-syncs if the text changes by any route that bypassed our callbacks.
            _poll = _field.schedule.Execute(Tick).Every(33);

            FailureReason = string.Empty;
            return true;
        }

        public void Detach()
        {
            _poll?.Pause();
            _poll = null;

            if (_inner != null)
            {
                _inner.UnregisterCallback<GeometryChangedEvent>(OnInnerGeometryChanged);
                _inner.style.color = StyleKeyword.Null; // back to the stylesheet colour
            }

            _overlay?.RemoveFromHierarchy();
            _overlay = null;
            _inner   = null;
            _lastPlain = null;
        }

        private TextElement? FindInnerTextElement()
        {
            var byClass = _field.Q<TextElement>(className: InnerClassName);
            if (byClass != null) return byClass;

            // Fallback: first TextElement under the input that isn't a scroller button.
            var input = _field.Q(TextField.textInputUssName);
            if (input == null) return null;

            foreach (var candidate in input.Query<TextElement>().ToList())
            {
                if (candidate is Button || candidate is RepeatButton) continue;
                return candidate;
            }

            return null;
        }

        // ── Content ───────────────────────────────────────────────────────────

        /// <summary>Re-render now (e.g. after the diagnostics marks changed).</summary>
        public void Refresh()
        {
            if (_overlay == null) return;
            _lastPlain = null;
            Rebuild(_field.value);
        }

        private void Rebuild(string plain)
        {
            if (_overlay == null || _buildRich == null) return;

            _lastPlain = plain;

            string rich;
            try
            {
                rich = _buildRich(plain);
            }
            catch (Exception ex)
            {
                // Never let a colouring bug make the text unreadable: with no rich
                // text at all the overlay would be blank AND the real glyphs hidden.
                Debug.LogException(ex);
                rich = MdixRichText.Escape(plain);
            }

            // TextElement.text is implemented through INotifyValueChanged<string>: assigning it
            // sends a ChangeEvent<string> that BUBBLES — through the input, up to the TextField
            // itself. The window's value-changed callback then read the overlay's colour markup
            // as if the person had typed it, and fed it to the language server, the compiler
            // and Save. Setting the value without notification sends nothing at all.
            if (_overlay is INotifyValueChanged<string> notifier)
                notifier.SetValueWithoutNotify(rich);
            else
                _overlay.text = rich;
        }

        /// <summary>
        /// True when <paramref name="target"/> is the colour layer. A change event whose target is
        /// this element is never an edit, however it got to the TextField.
        /// </summary>
        public bool OwnsEventTarget(object? target) =>
            _overlay != null && ReferenceEquals(target, _overlay);

        private void Tick()
        {
            if (_overlay == null || _inner == null) return;

            var current = _field.value;
            if (!ReferenceEquals(current, _lastPlain) && current != _lastPlain)
                Rebuild(current);

            var t = _inner.transform.position;
            if (t != _lastTranslation)
            {
                _lastTranslation = t;
                _overlay.transform.position = t;
            }
        }

        // ── Geometry ──────────────────────────────────────────────────────────

        private void OnInnerGeometryChanged(GeometryChangedEvent evt) => SyncBox();

        /// <summary>
        /// Gives the overlay exactly the inner element's box and text-affecting
        /// style, so its glyphs land on the same pixels as the (hidden) real ones.
        /// </summary>
        private void SyncBox()
        {
            if (_overlay == null || _inner == null) return;

            var r = _inner.layout;
            if (float.IsNaN(r.x) || float.IsNaN(r.y) || float.IsNaN(r.width) || float.IsNaN(r.height))
                return; // not laid out yet; the geometry event will call us again

            var s  = _overlay.style;
            var rs = _inner.resolvedStyle;

            s.left   = r.x;
            s.top    = r.y;
            s.width  = r.width;
            s.height = r.height;

            s.paddingLeft   = rs.paddingLeft;
            s.paddingTop    = rs.paddingTop;
            s.paddingRight  = rs.paddingRight;
            s.paddingBottom = rs.paddingBottom;

            s.borderLeftWidth   = rs.borderLeftWidth;
            s.borderTopWidth    = rs.borderTopWidth;
            s.borderRightWidth  = rs.borderRightWidth;
            s.borderBottomWidth = rs.borderBottomWidth;

            s.whiteSpace              = rs.whiteSpace;
            s.unityTextAlign          = rs.unityTextAlign;
            s.fontSize                = rs.fontSize;
            s.letterSpacing           = rs.letterSpacing;
            s.wordSpacing             = rs.wordSpacing;
            s.unityFontStyleAndWeight = rs.unityFontStyleAndWeight;
        }

        // ── Caret position (for popups) ───────────────────────────────────────

        /// <summary>
        /// Bottom-left corner of the caret, in <paramref name="root"/>'s local
        /// space. <c>cursorPosition</c> is expressed in the inner text element's
        /// coordinate space (it's what TextElementSelection paints the caret
        /// with), so converting through that element also accounts for
        /// padding and scroll translation.
        /// </summary>
        public bool TryGetCaretPosition(VisualElement root, out Vector2 position)
        {
            position = default;

            var inner = _inner ?? FindInnerTextElement();
            if (inner == null) return false;

            Vector2 local;
            try { local = _field.cursorPosition; }
            catch { return false; }

            if (float.IsNaN(local.x) || float.IsNaN(local.y)) return false;

            position = root.WorldToLocal(inner.LocalToWorld(local));
            return !(float.IsNaN(position.x) || float.IsNaN(position.y));
        }

        // ── Pointer → text offset (hover follows the mouse) ──────────────────────────
        //
        // UI Toolkit has no public "which character is under this point" call, so the
        // offset is found by measuring: the line from the vertical position, then the
        // column by binary search on the measured width of each prefix. The real input
        // element does the measuring (rich text off, same font), so proportional fonts
        // are handled. TextElement.MeasureTextSize is resolved by reflection so that a
        // Unity version without it only loses mouse hover instead of breaking the whole
        // editor assembly at compile time.

        private static System.Reflection.MethodInfo? _measure;
        private static object? _measureUndefined;
        private static bool _measureResolved;

        private static bool ResolveMeasure()
        {
            if (_measureResolved) return _measure != null;
            _measureResolved = true;

            try
            {
                var methods = typeof(TextElement).GetMethods(
                    System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Instance);

                foreach (var method in methods)
                {
                    if (method.Name != "MeasureTextSize" || method.ReturnType != typeof(Vector2)) continue;

                    var parameters = method.GetParameters();
                    if (parameters.Length != 5 || parameters[0].ParameterType != typeof(string)) continue;

                    _measureUndefined = Enum.ToObject(parameters[2].ParameterType, 0); // MeasureMode.Undefined
                    _measure          = method;
                    break;
                }
            }
            catch
            {
                _measure = null;
            }

            return _measure != null;
        }

        /// <summary>False when this Unity version can't measure text; hover then falls back to the caret.</summary>
        public static bool CanMapPointToOffset => ResolveMeasure();

        private static Vector2 MeasureSize(TextElement element, string text)
        {
            var result = _measure!.Invoke(
                element, new object[] { text, 0f, _measureUndefined!, 0f, _measureUndefined! });

            return result is Vector2 size ? size : default;
        }

        private static float MeasureWidth(TextElement element, string text) =>
            text.Length == 0 ? 0f : MeasureSize(element, text).x;

        /// <summary>
        /// The offset in <paramref name="text"/> of the character under a point given in panel
        /// (world) coordinates, or false when the point is not over any text: above the first
        /// line, below the last, or past the end of a line.
        /// </summary>
        public bool TryGetOffsetAtPoint(Vector2 worldPosition, string text, out int offset)
        {
            offset = -1;

            if (string.IsNullOrEmpty(text) || !ResolveMeasure()) return false;

            var inner = _inner ?? FindInnerTextElement();
            if (inner == null) return false;

            try
            {
                var local = inner.WorldToLocal(worldPosition);
                if (float.IsNaN(local.x) || float.IsNaN(local.y)) return false;

                var style = inner.resolvedStyle;
                var left  = style.paddingLeft + style.borderLeftWidth;
                var top   = style.paddingTop  + style.borderTopWidth;

                var lineHeight = MeasureSize(inner, "Ag").y;
                if (lineHeight < 1f) return false;

                var y = local.y - top;
                var x = local.x - left;
                if (y < 0f || x < 0f) return false;

                // Start of the line under the pointer.
                var lineIndex = (int)(y / lineHeight);
                var lineStart = 0;
                for (var i = 0; i < lineIndex; i++)
                {
                    var newline = text.IndexOf('\n', lineStart);
                    if (newline < 0) return false; // below the last line
                    lineStart = newline + 1;
                }

                var lineEnd = text.IndexOf('\n', lineStart);
                if (lineEnd < 0) lineEnd = text.Length;

                var line = text.Substring(lineStart, lineEnd - lineStart).TrimEnd('\r');
                if (line.Length == 0) return false;

                // Number of characters that lie completely left of the pointer.
                var low  = 0;
                var high = line.Length;
                while (low < high)
                {
                    var mid = (low + high + 1) / 2;
                    if (MeasureWidth(inner, line.Substring(0, mid)) <= x) low = mid;
                    else high = mid - 1;
                }

                if (low >= line.Length) return false; // past the end of the line

                offset = lineStart + low;
                return true;
            }
            catch
            {
                offset = -1;
                return false;
            }
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        private static Color ColorFromHex(string hex)
        {
            return ColorUtility.TryParseHtmlString(hex, out var c) ? c : Color.white;
        }
    }
}
