using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// Report a Problem, as the Linux app's (src/ui/report.rs, the engine's report): what happened (optional, up to
/// 2000 characters), exactly what will be sent (the app's recent log with your name, home folder and email
/// addresses taken out; the beta's own log after it), Send only once that's read; then the report's code to
/// quote, with Copy the Code. Nothing leaves without Send. Soft corners.
/// </summary>
internal static class ReportDialog
{
    public static async Task Show(EngineHub hub, XamlRoot root)
    {
        var w = Ffi.HelpWords();
        var note = new TextBox { PlaceholderText = w.ReportHeading, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MaxLength = 2000, Height = 96, CornerRadius = new CornerRadius(8) };
        var what = new TextBlock { TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true, FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Cascadia Mono, Consolas") , FontSize = 12 };
        var expander = new Expander { Header = w.ReportWhat, HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Stretch, Content = new ScrollViewer { Content = what, MaxHeight = 220 } };
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] };
        var form = new StackPanel { Spacing = 10, Width = 520 };
        form.Children.Add(Pages.PageKit.Note(w.ReportIntro));
        form.Children.Add(note);
        form.Children.Add(expander);
        form.Children.Add(status);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.ReportTitle,
            Content = form,
            PrimaryButtonText = w.Send,
            CloseButtonText = w.Cancel,
            IsPrimaryButtonEnabled = false,
            DefaultButton = ContentDialogButton.Primary, // (Send is the main one, as the Linux app's)
            CornerRadius = new CornerRadius(16),
        };
        dialog.Resources["ContentDialogMaxWidth"] = 640.0;
        var sent = false;

        void Render()
        {
            var r = hub.Engine.Report();
            what.Text = r.What;
            dialog.IsPrimaryButtonEnabled = r.Ready && !r.Sending && !sent;
            status.Text = r.Sending ? w.Sending : r.Failed ?? "";
            if (r.Code is { } code && !sent)
            {
                sent = true;
                var done = new StackPanel { Spacing = 10, Width = 520 };
                done.Children.Add(new TextBlock { Text = w.SentTitle, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
                done.Children.Add(Pages.PageKit.Note(r.SentText));
                done.Children.Add(Pages.PageKit.IconButton("", w.CopyCode, () =>
                {
                    var data = new Windows.ApplicationModel.DataTransfer.DataPackage();
                    data.SetText(code);
                    Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(data);
                }));
                dialog.Content = done;
                dialog.PrimaryButtonText = "";
                dialog.CloseButtonText = w.Close;
            }
        }

        void OnChange(ChangeKind kind)
        {
            if (kind == ChangeKind.Report) Render();
        }

        dialog.PrimaryButtonClick += (_, e) =>
        {
            e.Cancel = true; // it stays open: sending, then the code
            hub.Engine.ReportSend(note.Text);
        };
        hub.Change += OnChange;
        hub.Engine.ReportGather();
        Render();
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            hub.Change -= OnChange;
        }
    }
}
