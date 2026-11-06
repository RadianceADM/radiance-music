using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using RadianceMusic.Native;

namespace RadianceMusic.Pages;

public sealed partial class NowPlayingPage : Page
{
    /// <summary>Under these widths of the page (px; the GTK app's window breakpoints, 1060 and 860, less
    /// the sidebar): no lyrics column, a smaller cover.</summary>
    private const double Snug = 780, Narrow = 600;
    private EngineHub? hub;
    private string songKey = "";
    /// <summary>The music video's swapchain on the panel (0: none, the cover shows), and fullscreen.</summary>
    private nint shown;
    private bool full;

    /// <summary>A music video is on the panel (F11 and double-click make it fullscreen).</summary>
    public bool HasVideo => shown != 0;

    /// <summary>Whether the page shows its lyrics column (not under Snug), and when that changes.</summary>
    public bool ShowsLyrics { get; private set; } = true;
    public event Action? ShowsLyricsChanged;

    public NowPlayingPage()
    {
        InitializeComponent();
        SizeChanged += (_, e) => Fit(e.NewSize.Width);
        // the swapchain's buffers follow the panel (its size in DIPs: the spike's ruling)
        VideoPanel.SizeChanged += (_, _) => FitVideo();
        VideoPanel.CompositionScaleChanged += (_, _) => FitVideo();
        ToolTipService.SetToolTip(FullscreenButton, "Fullscreen (F11)");
    }

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        if (hub == null && e.Parameter is EngineHub h)
        {
            hub = h;
            // the lyrics column: word by word, growing toward the middle (Controls/LyricsColumn)
            Right.Children.Clear();
            Right.Children.Add(new Controls.LyricsColumn(hub, big: true));
            hub.Change += kind =>
            {
                if (kind == ChangeKind.Now) RenderNow();
            };
        }
        RenderNow();
    }

    private void Fit(double width)
    {
        var lyrics = width >= Snug;
        Right.Visibility = lyrics ? Visibility.Visible : Visibility.Collapsed;
        if (lyrics != ShowsLyrics)
        {
            ShowsLyrics = lyrics;
            ShowsLyricsChanged?.Invoke();
        }
        if (full)
        {
            // fullscreen: the video alone, the whole window
            Right.Visibility = Visibility.Collapsed;
            Grid.SetColumnSpan(Left, 2);
            Left.Width = Art.Width = Math.Max(1, Root.ActualWidth);
            Art.Height = Math.Max(1, Root.ActualHeight);
            return;
        }
        Left.HorizontalAlignment = lyrics ? HorizontalAlignment.Left : HorizontalAlignment.Center;
        Grid.SetColumnSpan(Left, lyrics ? 1 : 2);
        var side = width < Narrow ? Math.Max(160, Math.Min(340, width - 96)) : 340;
        // a music video is wider (16:9), as far as the room beside the lyrics lets it
        var (w, h) = ArtFit.Of(side, width - 72 - (lyrics ? 48 + 320 : 0), shown != 0);
        Left.Width = Art.Width = w;
        Art.Height = h;
    }

    /// <summary>The video's buttons: the version switch (both ways) and fullscreen.</summary>
    private void OnVideo(object sender, RoutedEventArgs e)
    {
        if (hub != null) hub.Engine.SwitchVersion(!hub.Engine.Now().Video);
    }

    private void OnFullscreen(object sender, RoutedEventArgs e) => App.Window?.SetVideoFullscreen(!full);

    private void OnVideoDoubleTapped(object sender, Microsoft.UI.Xaml.Input.DoubleTappedRoutedEventArgs e) => App.Window?.SetVideoFullscreen(!full);

    /// <summary>The window went fullscreen for the video, or came back (MainWindow.SetVideoFullscreen).</summary>
    public void SetFullscreen(bool on)
    {
        full = on;
        Body.Padding = on ? new Thickness(0) : new Thickness(36, 24, 36, 24);
        Art.Margin = on ? new Thickness(0) : new Thickness(0, 0, 0, 8);
        var words = on ? Visibility.Collapsed : Visibility.Visible;
        SongTitle.Visibility = Byline.Visibility = words;
        if (on) Quality.Visibility = Visibility.Collapsed;
        Back.Visibility = words;
        Root.Background = on ? new SolidColorBrush(Microsoft.UI.Colors.Black) : null;
        FullscreenIcon.Glyph = on ? "\uE73F" : "\uE740";
        ToolTipService.SetToolTip(FullscreenButton, on ? "Leave fullscreen (Esc)" : "Fullscreen (F11)");
        if (!on) RenderNow();
        Fit(ActualWidth);
    }

    /// <summary>The engine's swapchain on the panel, or none (0 lets the last frame go: no stale picture).</summary>
    private void ShowVideo(ulong? swapchain)
    {
        var ptr = (nint)(long)(swapchain ?? 0);
        if (ptr == shown) return;
        shown = ptr;
        Interop.SwapChainPanelNative.SetSwapChain(VideoPanel, ptr);
        var on = ptr != 0;
        VideoPanel.Visibility = VideoTaps.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        Cover.Visibility = on ? Visibility.Collapsed : Visibility.Visible;
        if (!on && full) App.Window?.SetVideoFullscreen(false);
        Fit(ActualWidth);
        FitVideo();
    }

    private void FitVideo()
    {
        if (hub == null || shown == 0 || VideoPanel.ActualWidth < 1 || VideoPanel.ActualHeight < 1) return;
        hub.Engine.ResizeVideo((uint)VideoPanel.ActualWidth, (uint)VideoPanel.ActualHeight);
    }

    private void RenderNow()
    {
        if (hub == null) return;
        var now = hub.Engine.Now();
        var song = now.Song;
        if (song == null)
        {
            SongTitle.Text = "Nothing playing";
            Byline.Text = "";
            Quality.Visibility = Visibility.Collapsed;
            Cover.Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"];
            Back.SetArt(null, null);
            OverArt(false);
            songKey = "";
            ShowVideo(null);
            VideoButton.Visibility = Visibility.Collapsed;
            FullscreenButton.Visibility = Visibility.Collapsed;
            return;
        }
        SongTitle.Text = song.Title;
        Byline.Text = song.Byline;
        // the GTK app's quality line (core::quality's words, from the engine)
        Quality.Visibility = full ? Visibility.Collapsed : Visibility.Visible;
        QualityText.Text = now.Quality.Length > 0 ? now.Quality : now.Lossless ? "Lossless · FLAC" : "Compressed · Opus";
        // a music video on the panel; the switch between the song and its video (either way)
        ShowVideo(hub.Engine.VideoSwapchain());
        // only where the other version was found: a video with no song (a cover, say) has nowhere to go
        VideoButton.Visibility = now.HasCounterpart ? Visibility.Visible : Visibility.Collapsed;
        VideoIcon.Glyph = now.Video ? "\uEC4F" : "\uE714";
        ToolTipService.SetToolTip(VideoButton, now.Video ? "Back to the song" : "Watch the music video");
        FullscreenButton.Visibility = shown != 0 ? Visibility.Visible : Visibility.Collapsed;
        if (song.Key != songKey)
        {
            songKey = song.Key;
            _ = LoadArt(song.Art);
        }
    }

    private async Task LoadArt(string source)
    {
        if (hub == null) return;
        var key = songKey;
        var cover = await ArtCache.LoadAsync(hub.Engine, source, 720);
        if (key != songKey) return;
        Cover.Background = cover != null ? new ImageBrush { ImageSource = cover, Stretch = Stretch.UniformToFill } : (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"];
        var files = await Task.Run(() => hub.Engine.Backdrop(source));
        if (key != songKey) return;
        Back.SetArt(files?.Wash, files?.Blob);
        OverArt(files != null);
    }

    /// <summary>Over the moving art the page is dark (its words stay readable); else Windows' own theme.</summary>
    private void OverArt(bool on)
    {
        Scrim.Visibility = on ? Visibility.Visible : Visibility.Collapsed;
        Root.RequestedTheme = on ? ElementTheme.Dark : ElementTheme.Default;
    }
}
