using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using RadianceMusic.Native;
using Ffi = RadianceMusic.Native.Native;

namespace RadianceMusic.Dialogs;

/// <summary>
/// Servers…, as the Linux app's (src/ui/rms.rs servers_dialog_with; stage 4c): what a Radiance Music Server is, Join a
/// Server (the invite link, Join, the line under it: Joining… / joined / on it already / moved / what went wrong), and
/// Your Servers: each one's state, what its owner lets you do there (Yes, or Off for you), Open Its Page and Leave (it
/// asks first). The joined list is the one today's Windows app keeps too (settings.json).
/// </summary>
internal static class ServersDialog
{
    /// <summary>Opens it (`invite`: a link to fill in, from a radiancemusic:// invite, Join focused).</summary>
    public static async Task Show(EngineHub hub, XamlRoot root, string? invite = null)
    {
        var w = Ffi.ServerWords();
        var secondary = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
        var caption = (Style)Application.Current.Resources["CaptionTextBlockStyle"];
        var form = new StackPanel { Spacing = 14, Width = 520 };
        form.Children.Add(new TextBlock { Text = w.Intro, TextWrapping = TextWrapping.Wrap, Foreground = secondary });

        // Join a Server
        form.Children.Add(new TextBlock { Text = w.JoinTitle, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
        form.Children.Add(new TextBlock { Text = w.JoinDescription, TextWrapping = TextWrapping.Wrap, Style = caption, Foreground = secondary });
        var link = new TextBox { PlaceholderText = w.InviteLink, Text = invite ?? "" };
        var join = new Button { Content = Labelled("", w.Join), Style = (Style)Application.Current.Resources["AccentButtonStyle"] };
        var line = new Grid { ColumnSpacing = 8 };
        line.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        line.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        line.Children.Add(link);
        Grid.SetColumn(join, 1);
        line.Children.Add(join);
        form.Children.Add(line);
        var note = new TextBlock { TextWrapping = TextWrapping.Wrap, Visibility = Visibility.Collapsed };
        form.Children.Add(note);
        void Join()
        {
            if (link.Text.Trim().Length == 0) return;
            hub.Engine.ServerJoin(link.Text.Trim());
        }
        join.Click += (_, _) => Join();
        link.KeyDown += (_, e) =>
        {
            if (e.Key != Windows.System.VirtualKey.Enter) return;
            e.Handled = true;
            Join();
        };

        // Your Servers
        form.Children.Add(new TextBlock { Text = w.YourServers, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"], Margin = new Thickness(0, 8, 0, 0) });
        var rows = new StackPanel { Spacing = 8 };
        form.Children.Add(rows);

        var dialog = new ContentDialog
        {
            XamlRoot = root,
            Title = w.Title,
            Content = new ScrollViewer { Content = form, VerticalScrollBarVisibility = ScrollBarVisibility.Auto },
            CloseButtonText = "Close",
            CornerRadius = new CornerRadius(16),
        };
        dialog.Resources["ContentDialogMaxWidth"] = 640.0;

        var shown = "";
        void Render()
        {
            var s = hub.Engine.Servers();
            join.IsEnabled = !s.Joining;
            note.Text = s.Note;
            note.Visibility = s.Note.Length > 0 ? Visibility.Visible : Visibility.Collapsed;
            note.Foreground = s.NoteError ? (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"] : secondary;
            if (s.Note.StartsWith("Joined ")) link.Text = "";
            // the rows made again only when something in them changed (an open row stays open)
            var now = string.Join("\n", s.Rows.Select(r => $"{r.Address}|{r.Name}|{r.Subtitle}|{r.Known}|{string.Join(",", r.Permissions.Select(p => p.Yes))}"));
            if (now == shown) return;
            shown = now;
            rows.Children.Clear();
            if (s.Rows.Length == 0)
            {
                rows.Children.Add(new TextBlock { Text = w.NoServers, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
                rows.Children.Add(new TextBlock { Text = w.NoServersSub, Style = caption, Foreground = secondary });
                return;
            }
            foreach (var r in s.Rows) rows.Children.Add(Row(hub, w, r, dialog));
        }
        void OnChange(ChangeKind kind)
        {
            if (kind == ChangeKind.Servers) Render();
        }
        hub.Change += OnChange;
        dialog.Closed += (_, _) => hub.Change -= OnChange;
        Render();
        if (invite != null) dialog.Opened += (_, _) => join.Focus(FocusState.Programmatic);
        await dialog.ShowAsync();
    }

    /// <summary>A joined server: its name and state; opened, what you can do there, Open Its Page and Leave.</summary>
    private static Expander Row(EngineHub hub, ServerWords w, ServerRowView r, ContentDialog dialog)
    {
        var secondary = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
        var head = new StackPanel { Spacing = 2 };
        head.Children.Add(new TextBlock { Text = r.Name, Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
        head.Children.Add(new TextBlock { Text = r.Subtitle, TextWrapping = TextWrapping.Wrap, Style = (Style)Application.Current.Resources["CaptionTextBlockStyle"], Foreground = r.Problem != null ? (Brush)Application.Current.Resources["SystemFillColorCautionBrush"] : secondary });
        var body = new StackPanel { Spacing = 6 };
        foreach (var p in r.Permissions)
        {
            var g = new Grid();
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            g.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            g.Children.Add(new TextBlock { Text = p.Title });
            var answer = new TextBlock { Text = r.Known ? p.Answer : "…", Foreground = p.Yes ? (Brush)Application.Current.Resources["TextFillColorPrimaryBrush"] : secondary };
            Grid.SetColumn(answer, 1);
            g.Children.Add(answer);
            body.Children.Add(g);
        }
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Margin = new Thickness(0, 6, 0, 0) };
        var open = new Button { Content = Labelled("", w.OpenPage) };
        ToolTipService.SetToolTip(open, w.OpenPageTip);
        open.Click += (_, _) =>
        {
            dialog.Hide();
            App.Window?.Show("server:" + r.Address);
        };
        var leave = new Button { Content = Labelled("", w.Leave) };
        ToolTipService.SetToolTip(leave, w.LeaveTip);
        // Leave asks first (a flyout: a dialog can't open over this one)
        var ask = Ffi.ServerLeaveWords(r.Name);
        var confirm = new StackPanel { Spacing = 10, MaxWidth = 340 };
        confirm.Children.Add(new TextBlock { Text = ask[0], Style = (Style)Application.Current.Resources["BodyStrongTextBlockStyle"] });
        confirm.Children.Add(new TextBlock { Text = ask[1], TextWrapping = TextWrapping.Wrap });
        var sure = new Button { Content = Labelled("", w.Leave), Background = (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"], Foreground = new SolidColorBrush(Microsoft.UI.Colors.White) };
        var cancel = new Button { Content = Labelled("", w.Cancel) };
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, HorizontalAlignment = HorizontalAlignment.Right };
        row.Children.Add(cancel);
        row.Children.Add(sure);
        confirm.Children.Add(row);
        var flyout = new Flyout { Content = confirm };
        FlyoutBase.SetAttachedFlyout(leave, flyout);
        leave.Click += (_, _) => FlyoutBase.ShowAttachedFlyout(leave);
        cancel.Click += (_, _) => flyout.Hide();
        sure.Click += (_, _) =>
        {
            flyout.Hide();
            leave.IsEnabled = false;
            hub.Engine.ServerLeave(r.Address);
        };
        buttons.Children.Add(open);
        buttons.Children.Add(leave);
        body.Children.Add(buttons);
        return new Expander { Header = head, Content = body, HorizontalAlignment = HorizontalAlignment.Stretch, HorizontalContentAlignment = HorizontalAlignment.Stretch };
    }

    private static StackPanel Labelled(string glyph, string text)
    {
        var p = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        p.Children.Add(new FontIcon { Glyph = glyph, FontSize = 14 });
        p.Children.Add(new TextBlock { Text = text });
        return p;
    }
}
