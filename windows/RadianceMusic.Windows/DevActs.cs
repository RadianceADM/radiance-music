using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace RadianceMusic;

/// <summary>
/// --act STEP (developer screenshots on ribs03, through --shot): what a person would do before
/// the shot, done by the app itself, so no clicks or keys go to the owner's desktop:
///   like:TITLE    likes Home's recent song of that title (if it isn't already)
///   menu          opens the menu of the song row highest on screen
///   click:TEXT    presses the button or menu item whose words (or Narrator name) start with TEXT (a menu's or dialog's first)
///   group:KIND:NAME   an album's, artist's or genre's songs (KIND: albums, artists, genres)
///   switch:video|song the music video or the song (the art's button); fullscreen: the video fullscreen
///   wait:N        N seconds more before the next step (Shot)
///   liked-search:TEXT  typed into Liked Songs' own search; queue-move:FROM:TO  the queue's drop
///   (rows of its list); queue-menu:N  that queue row's menu; lyric-tap:N  a tap on lyric line N;
///   next          the next song (the bar's Next); mixer-open  the bar's Mixer pop-up opened;
///   part:NAME:GAIN  a Mixer slider (0..1); splitting  the Splitting… dialog
///   voice:song|novocals|vocals  a Mixer voice button; mix-log  the Mixer's state in the log (voice, splitting, parts)
///   transition:off|crossfade|dj, fade:N  Between Songs (Settings → Playing); seek-end:N  N seconds before the song's end
///   share:to|from  Share Songs with a Phone open that way; share-log  its state in the log
///   servers[:INVITE]  Servers… (an invite filled in); join:INVITE  joins by it; server-first  the first joined server's page;
///   servers-log  the servers and the first one's page in the log; server-play  its first song; server-leave  leaves it
/// </summary>
internal static class DevActs
{
    public static void Run(MainWindow window, string act)
    {
        var colon = act.IndexOf(':');
        var (what, arg) = colon >= 0 ? (act[..colon], act[(colon + 1)..]) : (act, "");
        var done = what switch
        {
            "like" => Like(arg),
            "menu" => window.Content is { } root && Controls.SongMenu.OpenFirst(root),
            "click" => window.Content?.XamlRoot is { } xr && Click(xr, window.Content, arg),
            "group" => Group(window, arg),
            "switch" => Switch(arg),
            "liked-search" => window.Page is Pages.LikedPage liked && Do(() => liked.SearchFor(arg)),
            "queue-move" => arg.Split(':') is [var a, var b] && int.TryParse(a, out var from) && int.TryParse(b, out var to) && window.SidePanel.Drop(from, to),
            "queue-menu" => int.TryParse(arg, out var row) && window.SidePanel.OpenMenu(row),
            // the queue's order now, in the log (a real drag's check: before and after)
            "queue-where" => int.TryParse(arg, out var qr) && Do(() => Crash.Note($"queue row {qr} at {window.SidePanel.Where(qr)}")),
            "play-song" => window.Page is Pages.SearchPage ps && ps.PlayFirstSong(),
            "queue-log" => Do(() => Crash.Note("queue now: " + window.SidePanel.Titles())),
            "lyric-tap" => window.Page is Pages.NowPlayingPage np && int.TryParse(arg, out var line) && np.TapLyric(line),
            "fullscreen" => Fullscreen(window),
            "next" => Do(() => App.Hub.Engine.Skip(1)),
            "mixer-open" => window.Bar?.OpenMixer() == true,
            "splitting" => window.Content?.XamlRoot is { } sx && Do(() => _ = Dialogs.SplittingDialog.Open(App.Hub, sx)),
            "voice" => arg switch { "song" => Native.VoiceMode.Song, "novocals" => Native.VoiceMode.NoVocals, "vocals" => Native.VoiceMode.VocalsOnly, _ => (Native.VoiceMode?)null } is { } vm && Do(() => App.Hub.Engine.SetVoice(vm)),
            "transition" => arg switch { "crossfade" => Native.TransitionMode.Crossfade, "dj" => Native.TransitionMode.DjMix, "off" => Native.TransitionMode.Off, _ => (Native.TransitionMode?)null } is { } tm && Do(() => App.Hub.Engine.SetTransition(tm)),
            "share" => window.Content?.XamlRoot is { } shx && Do(() => _ = Dialogs.ShareDialog.Show(App.Hub, shx, arg != "from")),
            // stage 4c: Servers…, joining by invite, a server's page (the first joined), the servers in the log
            "servers" => window.Content?.XamlRoot is { } svx && Do(() => _ = Dialogs.ServersDialog.Show(App.Hub, svx, arg.Length > 0 ? arg : null)),
            "join" => Do(() => App.Hub.Engine.ServerJoin(arg)),
            "server-first" => App.Hub.Engine.Servers().Rows is { Length: > 0 } sr && Do(() => window.Show("server:" + sr[0].Address)),
            "server-play" => App.Hub.Engine.Servers().Rows is { Length: > 0 } sp0 && App.Hub.Engine.ServerPage(sp0[0].Address, "").Songs is { Length: > 0 } sps && Do(() => App.Hub.Engine.PlayServer(sp0[0].Address, "", 0, sps[0].Key)),
            "server-leave" => App.Hub.Engine.Servers().Rows is { Length: > 0 } sl && Do(() => App.Hub.Engine.ServerLeave(sl[0].Address)),
            "servers-log" => Do(() => { var s = App.Hub.Engine.Servers(); Crash.Note($"servers: [{string.Join("; ", s.Rows.Select(r => $"{r.Name} {r.Address} '{r.Subtitle}' known {r.Known} problem '{r.Problem}'"))}] note '{s.Note}' (error {s.NoteError})"); if (s.Rows.Length > 0) { var p = App.Hub.Engine.ServerPage(s.Rows[0].Address, ""); Crash.Note($"server page: {p.Name} '{p.Line}' '{p.Permissions}' songs [{string.Join(", ", p.Songs.Select(x => x.Title + " / " + x.Subtitle))}] note '{p.Note}'"); } }),
            "share-log" => Do(() => { var s = App.Hub.Engine.Share(); Crash.Note($"share: on {s.On}, to {s.ToPhone}, qr '{s.Qr}', caption '{s.Caption}', progress '{s.Progress}', firewall {s.Firewall}"); }),
            "seek-end" => double.TryParse(arg, System.Globalization.CultureInfo.InvariantCulture, out var before) && App.Hub.Engine.Now() is { Length: > 0 } sn && Do(() => App.Hub.Engine.Seek(Math.Max(0, sn.Length - before))),
            "fade" => double.TryParse(arg, System.Globalization.CultureInfo.InvariantCulture, out var fs) && Do(() => App.Hub.Engine.SetFade(fs)),
            "mix-log" => Do(() => { var m = App.Hub.Engine.Now(); var sp = App.Hub.Engine.MixerSplit(); Crash.Note($"mix: {m.Song?.Title}: voice {m.Mix.Voice}, splitting '{sp.Line}' ({sp.Progress:F2}, turn off {sp.TurnOff}), choices [{string.Join(", ", sp.Items.Select(i => i.Label))}], note '{m.Mix.Note}', parts {m.Mix.Parts.Length}, plays parts {m.Mix.PlaysParts}"); }),
            "part" => arg.Split(':') is [var name, var g] && double.TryParse(g, System.Globalization.CultureInfo.InvariantCulture, out var gain) && Do(() => App.Hub.Engine.SetPart(name, gain)),
            // stage 4a: the Settings page's sections, the app's own pop-ups, a Storage move's confirm
            "settings" => int.TryParse(arg, out var sect) && Do(() => { window.Show("settings"); if (window.Page is Pages.SettingsPage sp) sp.Section = sect; }),
            "guide" => window.Content?.XamlRoot is { } gx && Do(() => _ = Dialogs.GuideDialog.Show(App.Hub, gx)),
            "whats-new" => window.Content?.XamlRoot is { } nx && Do(() => _ = Dialogs.WhatsNewDialog.Show(nx)),
            "update" => window.Content?.XamlRoot is { } ux && Do(() => _ = Dialogs.UpdateDialog.Show(App.Hub, ux)),
            "report" => window.Content?.XamlRoot is { } rx && Do(() => _ = Dialogs.ReportDialog.Show(App.Hub, rx)),
            "rating" => window.Content?.XamlRoot is { } fx && Do(() => _ = Dialogs.RatingDialog.Show(App.Hub, fx, false)),
            "storage-move" => window.Page is Pages.SettingsPage mp && Do(() => _ = mp.ConfirmMove(App.Hub.Engine.Storage().Folder, arg)),
            "suggest" => window.Page is Pages.SearchPage spg && Do(() => spg.Typed(arg)),
            _ => false,
        };
        Crash.Note($"act: {act}: {(done ? "done" : "not found")}");
    }

    /// <summary>group:albums|artists|genres:NAME: that album's, artist's or genre's songs.</summary>
    private static bool Group(MainWindow window, string arg)
    {
        var parts = arg.Split(':', 2);
        if (parts.Length != 2) return false;
        var kind = parts[0] switch { "albums" => Native.GroupKind.Albums, "artists" => Native.GroupKind.Artists, _ => Native.GroupKind.Genres };
        window.Show("group", (kind, parts[1]));
        return true;
    }

    private static bool Do(Action a)
    {
        a();
        return true;
    }

    /// <summary>switch:video or switch:song: the version switch, as the button on the art.</summary>
    private static bool Switch(string to)
    {
        App.Hub.Engine.SwitchVersion(to == "video");
        return true;
    }

    private static bool Fullscreen(MainWindow window)
    {
        window.SetVideoFullscreen(true);
        return window.VideoFullscreen;
    }

    private static bool Like(string title)
    {
        if (App.Hub.Engine.Home().Recent.FirstOrDefault(s => s.Title == title) is not { } song) return false;
        if (!song.Liked) App.Hub.Engine.ToggleLike(song.Key);
        return true;
    }

    /// <summary>The open pop-ups first (a menu or dialog is on top), then the window.</summary>
    private static bool Click(XamlRoot xr, UIElement content, string text)
    {
        var roots = VisualTreeHelper.GetOpenPopupsForXamlRoot(xr).Select(p => (DependencyObject)p.Child).Where(c => c != null).ToList();
        roots.Add(content);
        foreach (var root in roots)
            if (Find(root, text) is { } element && FrameworkElementAutomationPeer.CreatePeerForElement(element) is { } peer)
            {
                // a submenu opens (Expand); a button or menu item is pressed (Invoke)
                if (peer.GetPattern(PatternInterface.ExpandCollapse) is IExpandCollapseProvider e) e.Expand();
                else if (peer.GetPattern(PatternInterface.Invoke) is IInvokeProvider i) i.Invoke();
                else if (peer.GetPattern(PatternInterface.Toggle) is IToggleProvider t) t.Toggle(); // (Tap Timing)
                else return false;
                return true;
            }
        return false;
    }

    private static UIElement? Find(DependencyObject at, string text)
    {
        switch (at)
        {
            case MenuFlyoutSubItem sub when sub.Text.StartsWith(text): return sub;
            case MenuFlyoutItem item when item.Text.StartsWith(text): return item;
            case Microsoft.UI.Xaml.Controls.Primitives.ButtonBase b when Words(b).StartsWith(text) || Microsoft.UI.Xaml.Automation.AutomationProperties.GetName(b).StartsWith(text): return b;
        }
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(at); i++)
            if (Find(VisualTreeHelper.GetChild(at, i), text) is { } found) return found;
        return null;
    }

    /// <summary>A button's words (its content's text blocks, or its string).</summary>
    private static string Words(ContentControl b) => b.Content switch
    {
        string s => s,
        TextBlock t => t.Text,
        DependencyObject d => string.Join(" ", Texts(d)),
        _ => "",
    };

    private static IEnumerable<string> Texts(DependencyObject d)
    {
        if (d is TextBlock t && t.Text.Length > 0) yield return t.Text;
        if (d is Panel p)
            foreach (var c in p.Children)
                foreach (var s in Texts(c)) yield return s;
    }
}
