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

            _overlay.text = rich;
        }

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

        // ── Helpers ───────────────────────────────────────────────────────────

        private static Color ColorFromHex(string hex)
        {
            return ColorUtility.TryParseHtmlString(hex, out var c) ? c : Color.white;
        }
    }
}
