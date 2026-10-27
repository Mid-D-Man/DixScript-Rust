using System;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// A modal card over the MDIX window: a title, any content, an error line and Apply / Cancel.
    ///
    /// One is open at a time. Esc cancels. Enter applies, except for a text area, where only Ctrl or Cmd
    /// with Enter does (<c>enterApplies: false</c>). Apply runs the callback; when that returns an error
    /// message the card stays open and shows it, otherwise the card closes. The scrim behind the card
    /// takes every click, so the window below cannot be used while the card is open.
    /// </summary>
    internal sealed class MdixPopup
    {
        private readonly VisualElement _scrim;
        private readonly Label         _error;
        private readonly Button        _apply;
        private readonly Func<string>  _onApply;       // returns an error message, or null when done
        private readonly bool          _enterApplies;

        private bool _open;
        private bool _applying;

        public bool IsOpen { get { return _open; } }

        private MdixPopup(string title, string subtitle, VisualElement body, Func<string> onApply, string applyLabel, bool enterApplies)
        {
            _onApply      = onApply;
            _enterApplies = enterApplies;

            // Layout is set here so the card is placed even if the stylesheet fails to load; the look is in the USS.
            _scrim = new VisualElement { focusable = true };
            _scrim.AddToClassList("mdix-pop__scrim");
            _scrim.style.position       = Position.Absolute;
            _scrim.style.left           = 0;
            _scrim.style.top            = 0;
            _scrim.style.right          = 0;
            _scrim.style.bottom         = 0;
            _scrim.style.justifyContent = Justify.Center;
            _scrim.style.alignItems     = Align.Center;

            var card = new VisualElement();
            card.AddToClassList("mdix-pop");

            var titleLabel = new Label(title);
            titleLabel.AddToClassList("mdix-pop__title");
            card.Add(titleLabel);

            if (!string.IsNullOrEmpty(subtitle))
            {
                var sub = new Label(subtitle);
                sub.AddToClassList("mdix-pop__subtitle");
                card.Add(sub);
            }

            card.Add(body);

            _error = new Label();
            _error.AddToClassList("mdix-pop__error");
            _error.style.display = DisplayStyle.None;
            card.Add(_error);

            var footer = new VisualElement();
            footer.AddToClassList("mdix-pop__footer");

            var cancel = new Button(RequestClose) { text = "Cancel" };
            cancel.AddToClassList("mdix-btn");
            footer.Add(cancel);

            _apply = new Button(RequestApply) { text = applyLabel };
            _apply.AddToClassList("mdix-btn");
            _apply.AddToClassList("mdix-btn--primary");
            footer.Add(_apply);

            card.Add(footer);
            _scrim.Add(card);

            _scrim.RegisterCallback<KeyDownEvent>(OnKeyDown, TrickleDown.TrickleDown);
        }

        /// <summary>
        /// Shows a card on <paramref name="host"/> (the window's root).
        /// <paramref name="onApply"/> returns null when the change was made, or the message to show when it was refused.
        /// </summary>
        public static MdixPopup Open(
            VisualElement host, string title, string subtitle, VisualElement body,
            Func<string> onApply, string applyLabel = "Apply", VisualElement focus = null, bool enterApplies = true)
        {
            var popup = new MdixPopup(title, subtitle, body, onApply, applyLabel, enterApplies) { _open = true };
            host.Add(popup._scrim);

            // Focus goes to the scrim at least, so Esc works whatever else is in the card.
            var target = focus ?? popup._scrim;
            popup._scrim.schedule.Execute(() => target.Focus()).ExecuteLater(10);
            return popup;
        }

        /// <summary>Removes the card. Safe to call twice.</summary>
        public void Close()
        {
            if (!_open) return;
            _open = false;
            _scrim.RemoveFromHierarchy();
        }

        /// <summary>Shows a message under the content, or hides the line when it is null or empty.</summary>
        public void SetError(string message)
        {
            var has = !string.IsNullOrEmpty(message);
            _error.text          = has ? message : string.Empty;
            _error.style.display = has ? DisplayStyle.Flex : DisplayStyle.None;
        }

        public void SetApplyEnabled(bool enabled)
        {
            _apply.SetEnabled(enabled);
        }

        // ── Keys and buttons ──────────────────────────────────────────────────

        private void OnKeyDown(KeyDownEvent evt)
        {
            if (!_open) return;

            if (evt.keyCode == KeyCode.Escape)
            {
                evt.StopImmediatePropagation();
                RequestClose();
                return;
            }

            if (evt.keyCode == KeyCode.Return || evt.keyCode == KeyCode.KeypadEnter)
            {
                if (evt.target is Button) return;       // Enter on a focused button presses that button

                var withModifier = (evt.modifiers & (EventModifiers.Control | EventModifiers.Command)) != 0;
                if (_enterApplies || withModifier)
                {
                    evt.StopImmediatePropagation();
                    RequestApply();
                }
            }
        }

        // Work that rebuilds the window is done after the event that asked for it has finished.
        private void RequestClose()
        {
            EditorApplication.delayCall += Close;
        }

        private void RequestApply()
        {
            EditorApplication.delayCall += RunApply;
        }

        private void RunApply()
        {
            if (!_open || _applying || !_apply.enabledSelf) return;

            _applying = true;
            try
            {
                var error = _onApply();
                if (error == null) Close();
                else               SetError(error);
            }
            finally
            {
                _applying = false;
            }
        }
    }
}
