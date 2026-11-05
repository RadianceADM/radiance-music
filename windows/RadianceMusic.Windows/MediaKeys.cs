using Windows.Media;
using Windows.Storage;
using Windows.Storage.Streams;

namespace RadianceMusic;

/// <summary>
/// The keyboard's media keys, the volume flyout's card and the lock screen (Windows' System Media
/// Transport Controls, as the GTK app's smtc.rs): the song with its cover, Play/Pause/Next/Previous,
/// and the time (at most every 5 s).
/// </summary>
internal sealed class MediaKeys
{
    private readonly EngineHub hub;
    private readonly SystemMediaTransportControls smtc;
    private string shown = "";
    private DateTime timeline = DateTime.MinValue;

    public MediaKeys(EngineHub hub, nint hwnd)
    {
        this.hub = hub;
        smtc = SystemMediaTransportControlsInterop.GetForWindow(hwnd);
        smtc.IsEnabled = true;
        smtc.ButtonPressed += (_, e) =>
        {
            // (Windows' thread: the engine takes commands from any thread)
            var playing = hub.Engine.Now().Playing;
            switch (e.Button)
            {
                case SystemMediaTransportControlsButton.Play when !playing:
                case SystemMediaTransportControlsButton.Pause when playing:
                    hub.Engine.TogglePlay();
                    break;
                case SystemMediaTransportControlsButton.Next: hub.Engine.Skip(1); break;
                case SystemMediaTransportControlsButton.Previous: hub.Engine.Skip(-1); break;
            }
        };
        hub.Change += kind =>
        {
            if (kind == Native.ChangeKind.Now) _ = Update();
            else if (kind == Native.ChangeKind.Position) Time();
        };
        _ = Update();
    }

    private async Task Update()
    {
        var now = hub.Engine.Now();
        var song = now.Song;
        smtc.IsPlayEnabled = song != null;
        smtc.IsPauseEnabled = song != null;
        smtc.IsNextEnabled = now.CanNext;
        smtc.IsPreviousEnabled = now.CanPrevious;
        smtc.PlaybackStatus = song == null ? MediaPlaybackStatus.Stopped : now.Playing ? MediaPlaybackStatus.Playing : MediaPlaybackStatus.Paused;
        var key = song?.Key ?? "";
        if (key != shown)
        {
            shown = key;
            var u = smtc.DisplayUpdater;
            u.ClearAll();
            if (song != null)
            {
                u.Type = MediaPlaybackType.Music;
                u.MusicProperties.Title = song.Title;
                u.MusicProperties.Artist = song.Byline;
                u.MusicProperties.AlbumTitle = song.Album;
                var art = await Task.Run(() => hub.Engine.ArtFile(song.Art, 300));
                if (art != null && key == shown)
                    u.Thumbnail = RandomAccessStreamReference.CreateFromFile(await StorageFile.GetFileFromPathAsync(art));
            }
            u.Update();
            timeline = DateTime.MinValue;
        }
        Time();
    }

    private void Time()
    {
        if (DateTime.UtcNow - timeline < TimeSpan.FromSeconds(5)) return;
        timeline = DateTime.UtcNow;
        var now = hub.Engine.Now();
        if (now.Song == null) return;
        smtc.UpdateTimelineProperties(new SystemMediaTransportControlsTimelineProperties
        {
            StartTime = TimeSpan.Zero,
            EndTime = TimeSpan.FromSeconds(now.Length),
            MinSeekTime = TimeSpan.Zero,
            MaxSeekTime = TimeSpan.FromSeconds(now.Length),
            Position = TimeSpan.FromSeconds(now.At),
        });
    }
}
