using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// Rate Radiance Music, as the Linux app's pop-up (src/ui/feedback.rs, the engine's feedback): "Are you enjoying
/// Radiance Music?" Yes! / Not really, five rows of stars, a comment; Send signs in with Google in the browser
/// the first time (the engine's loopback; the browser opened here), then sends; Not Now, Don't Ask Again; 4–5
/// stars, the thanks offer Share Radiance Music. By itself only at a free moment (the gate). Soft corners.
/// </summary>
internal static class RatingDialog
{
    public static bool Up { get; private set; }

    public static async Task Show(EngineHub hub, XamlRoot root, bool byItself)
    {
        if (Up) return;
        var w = Ffi.HelpWords();
        if (byItself) hub.Engine.RatingShown(); // not again for 30 days, whatever is pressed
        var yes = new ToggleButton { Content = w.Yes, CornerRadius = new CornerRadius(16) };
        var no = new ToggleButton { Content = w.NotReally, CornerRadius = new CornerRadius(16) };
        yes.Checked += (_, _) => no.IsChecked = false;
        no.Checked += (_, _) => yes.IsChecked = false;
        var pills = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        pills.Children.Add(yes);
        pills.Children.Add(no);
        var stars = new List<RatingControl>();
        var rows = new StackPanel { Spacing = 4 };
        foreach (var area in w.RateAreas)
        {
            var row = new Grid { ColumnSpacing = 12 };
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(140) });
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            var r = new RatingControl { MaxRating = 5, IsClearEnabled = false, Value = -1 };
            ToolTipService.SetToolTip(r, area);
            Grid.SetColumn(r, 1);
            row.Children.Add(new TextBlock { Text = area, VerticalAlignment = VerticalAlignment.Center });
            row.Children.Add(r);
            rows.Children.Add(row);
            stars.Add(r);
        }
        var comment = new TextBox { PlaceholderText = w.RateComment, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap, MaxLength = 2000, Height = 72, CornerRadius = new CornerRadius(8) };
        var who = new TextBlock { TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] };
        var privacy = new HyperlinkButton { Content = w.PrivacyWord, NavigateUri = new Uri(w.Privacy), Padding = new Thickness(0) };
        var signOut = new HyperlinkButton { Content = w.SignOut, Padding = new Thickness(0) };
        signOut.Click += (_, _) => hub.Engine.SignOut();
        var whoRow = new StackPanel { Spacing = 2 };
        whoRow.Children.Add(who);
        var links = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        links.Children.Add(privacy);
        links.Children.Add(signOut);
        whoRow.Children.Add(links);
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"] };
        var form = new StackPanel { Spacing = 10, Width = 460 };
        form.Children.Add(new TextBlock { Text = w.RateHead, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"], TextWrapping = TextWrapping.Wrap });
        form.Children.Add(Pages.PageKit.Note(w.RateSub));
        form.Children.Add(pills);
        form.Children.Add(rows);
        form.Children.Add(comment);
        form.Children.Add(whoRow);
        form.Children.Add(status);
        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.RateTitle,
            Content = form,
            PrimaryButtonText = w.Send,
            SecondaryButtonText = w.DontAsk,
            CloseButtonText = w.NotNow,
            DefaultButton = ContentDialogButton.None,
            CornerRadius = new CornerRadius(16),
        };
        string? opened = null;
        var thanked = false;

        void Render()
        {
            var r = hub.Engine.Rating();
            who.Text = r.Who + ".";
            signOut.Visibility = r.AccountName != null ? Visibility.Visible : Visibility.Collapsed;
            // the browser's sign-in: opened here, once
            if (r.SignInUrl is { } url && url != opened)
            {
                opened = url;
                _ = Windows.System.Launcher.LaunchUriAsync(new Uri(url));
            }
            var busy = r.Busy.Length > 0;
            dialog.IsPrimaryButtonEnabled = !busy;
            dialog.SecondaryButtonText = busy ? w.CancelSignIn : w.DontAsk;
            if (r.Outcome == RateOutcomeView.Failed) status.Text = r.Failed;
            else status.Text = r.Busy;
            if (r.Outcome == RateOutcomeView.Sent && !thanked)
            {
                thanked = true;
                var done = new StackPanel { Spacing = 10, Width = 460 };
                done.Children.Add(new TextBlock { Text = w.Thanks, Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
                done.Children.Add(Pages.PageKit.Note(r.Share ? w.ThanksShare : w.ThanksPlain));
                if (r.Share)
                    done.Children.Add(Pages.PageKit.IconButton("", w.ShareApp, () =>
                    {
                        var data = new Windows.ApplicationModel.DataTransfer.DataPackage();
                        data.SetText(w.ShareLink);
                        Windows.ApplicationModel.DataTransfer.Clipboard.SetContent(data);
                        App.Window?.Say(w.ShareToast);
                    }, true));
                dialog.Content = done;
                dialog.PrimaryButtonText = "";
                dialog.SecondaryButtonText = "";
                dialog.CloseButtonText = w.Done;
            }
        }

        void OnChange(ChangeKind kind)
        {
            if (kind == ChangeKind.Rating) Render();
        }

        dialog.PrimaryButtonClick += (_, e) =>
        {
            e.Cancel = true;
            if ((yes.IsChecked != true && no.IsChecked != true) || stars.Any(s => s.Value < 1))
            {
                status.Text = w.RateChoose;
                return;
            }
            hub.Engine.Rate(new AnswerView(yes.IsChecked == true, stars.Select(s => (byte)Math.Round(s.Value)).ToArray(), comment.Text));
        };
        dialog.SecondaryButtonClick += (_, e) =>
        {
            if (hub.Engine.Rating().Busy.Length > 0)
            {
                e.Cancel = true; // Cancel Sign-In
                hub.Engine.CancelSignIn();
            }
            else
            {
                hub.Engine.RatingNever(); // Don't Ask Again
            }
        };
        hub.Change += OnChange;
        Render();
        Up = true;
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
}
