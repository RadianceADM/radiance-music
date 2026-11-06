using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// Splitting… (stage 3), as the Linux app's dialog (src/ui/splitting.rs): splitting here is ready (Demucs splits,
/// MMS times the words, at idle priority) or what getting it ready is doing; the cores it may use; Pause Splitting.
/// Soft corners (the owner's: no sharp edges).
/// </summary>
internal static class SplittingDialog
{
    public static async Task Open(EngineHub hub, XamlRoot root)
    {
        var w = Ffi.BetaWords();
        var splits = hub.Engine.Splits();
        var cores = hub.Engine.SplitCores();
        var body = new StackPanel { Spacing = 16, MinWidth = 380 };
        var res = Application.Current.Resources;
        if (splits.CanSplit)
        {
            body.Children.Add(new TextBlock { Text = w.ReadyTitle, Style = (Style)res["BodyStrongTextBlockStyle"] });
            body.Children.Add(new TextBlock { Text = w.ReadyNote, Style = (Style)res["CaptionTextBlockStyle"], TextWrapping = TextWrapping.Wrap, Margin = new Thickness(0, -12, 0, 0) });
        }
        else
        {
            var setup = hub.Engine.Setup();
            body.Children.Add(new TextBlock { Text = setup.Text.Length > 0 ? setup.Text : w.SplittingDialog, TextWrapping = TextWrapping.Wrap });
        }
        var n = new NumberBox
        {
            Header = cores.Title,
            Description = new TextBlock { Text = cores.Note, TextWrapping = TextWrapping.Wrap, MaxWidth = 380, Style = (Style)res["CaptionTextBlockStyle"] }, // the note wraps (a plain Description runs off the edge)
            Minimum = 1,
            Maximum = Math.Max(1, cores.Max),
            Value = cores.Now,
            SmallChange = 1,
            SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Inline,
            IsEnabled = splits.CanSplit,
        };
        n.ValueChanged += (_, e) =>
        {
            if (!double.IsNaN(e.NewValue)) hub.Engine.SetSplitCores((uint)Math.Round(e.NewValue));
        };
        body.Children.Add(n);
        var pause = new ToggleSwitch { Header = w.PauseSplitting, IsOn = splits.Paused };
        pause.Toggled += (_, _) => hub.Engine.SetSplitPaused(pause.IsOn);
        body.Children.Add(pause);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.SplittingDialog.TrimEnd('…'),
            Content = body,
            CloseButtonText = "Close",
            CornerRadius = new CornerRadius(16),
        };
        await dialog.ShowAsync();
    }
}
