using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// The update pop-up, as the Linux app's (src/ui/update.rs, core's words): Out (what's new, Update Now with its
/// size, Remind Me Later, Skip This Version), Downloading (its bar; Hide, Cancel the Download), Ready (Restart to
/// Update), Restarting, Failed (Try Again, Get It from GitHub). It follows the steps while it's open; Esc on Out
/// or Ready is Remind Me Later. Its main actions carry their icons; soft corners.
/// </summary>
internal static class UpdateDialog
{
    public static bool Up { get; private set; }

    public static async Task Show(EngineHub hub, XamlRoot root)
    {
        if (Up) return;
        var w = Ffi.HelpWords();
        var body = new StackPanel { Spacing = 10, Width = 420 };
        var dialog = new ContentDialog { XamlRoot = root, Title = w.UpdateTitle, Content = body, CornerRadius = new CornerRadius(16) };
        UpdateStepView shown = UpdateStepView.None;

        Button Act(string glyph, string text, Action click, bool accent = false)
        {
            var b = Pages.PageKit.IconButton(glyph, text, click, accent);
            b.HorizontalAlignment = HorizontalAlignment.Stretch;
            return b;
        }

        void Render()
        {
            var u = hub.Engine.Update();
            shown = u.Step;
            body.Children.Clear();
            if (u.Step == UpdateStepView.None)
            {
                dialog.Hide();
                return;
            }
            var mark = new Border { Width = 56, Height = 56, CornerRadius = new CornerRadius(28), HorizontalAlignment = HorizontalAlignment.Left, Background = (Brush)Application.Current.Resources["AccentFillColorDefaultBrush"] };
            var glyph = u.Step switch { UpdateStepView.Downloading => "", UpdateStepView.Failed => "", UpdateStepView.Ready or UpdateStepView.Restarting => "", _ => "" };
            mark.Child = new FontIcon { Glyph = glyph, FontSize = 24, Foreground = (Brush)Application.Current.Resources["TextOnAccentFillColorPrimaryBrush"] };
            body.Children.Add(mark);
            body.Children.Add(new TextBlock { Text = u.Title, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
            body.Children.Add(Pages.PageKit.Note(u.Text));
            switch (u.Step)
            {
                case UpdateStepView.Out:
                    foreach (var b in u.Bullets) body.Children.Add(new TextBlock { Text = "•  " + b, TextWrapping = TextWrapping.Wrap });
                    body.Children.Add(Act(u.PageOnly ? "" : "", u.Main, () => Now(hub, u), true));
                    if (!u.PageOnly) body.Children.Add(Pages.PageKit.Note(w.KeepsPlaying));
                    body.Children.Add(Act("", w.SkipVersion, () => { hub.Engine.UpdateSkip(); dialog.Hide(); }));
                    dialog.CloseButtonText = w.RemindLater;
                    break;
                case UpdateStepView.Downloading:
                    body.Children.Add(new ProgressBar { Minimum = 0, Maximum = 1, Value = u.Done, CornerRadius = new CornerRadius(2) });
                    body.Children.Add(new TextBlock { Text = u.Progress, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] });
                    body.Children.Add(Act("", w.CancelDownload, () => hub.Engine.UpdateCancel()));
                    dialog.CloseButtonText = u.Main; // Hide: it goes on downloading
                    break;
                case UpdateStepView.Ready:
                    body.Children.Add(Act("", u.Main, () => hub.Engine.UpdateRestart(), true));
                    body.Children.Add(Act("", w.SkipVersion, () => { hub.Engine.UpdateSkip(); dialog.Hide(); }));
                    dialog.CloseButtonText = w.RemindLater;
                    break;
                case UpdateStepView.Restarting:
                    body.Children.Add(new ProgressRing { IsActive = true, Width = 32, Height = 32, HorizontalAlignment = HorizontalAlignment.Left });
                    dialog.CloseButtonText = "";
                    break;
                case UpdateStepView.Failed:
                    body.Children.Add(Act("", u.Main, () => hub.Engine.UpdateNow(), true));
                    body.Children.Add(Act("", w.FromGithub, () => _ = Windows.System.Launcher.LaunchUriAsync(new Uri(u.Page))));
                    dialog.CloseButtonText = w.RemindLater;
                    break;
            }
        }

        void OnChange(ChangeKind kind)
        {
            if (kind == ChangeKind.Update) Render();
        }

        dialog.CloseButtonClick += (_, _) =>
        {
            // Remind Me Later on Out, Ready or Failed; Hide while it downloads (it goes on)
            if (shown is UpdateStepView.Out or UpdateStepView.Ready or UpdateStepView.Failed) hub.Engine.UpdateLater();
        };
        Render();
        Up = true;
        hub.Change += OnChange;
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            hub.Change -= OnChange;
            Up = false;
        }
    }

    /// <summary>Update Now, or the release's page for a copy that can't put it on.</summary>
    private static void Now(EngineHub hub, UpdateView u)
    {
        if (u.PageOnly) _ = Windows.System.Launcher.LaunchUriAsync(new Uri(u.Page));
        else hub.Engine.UpdateNow();
    }
}
