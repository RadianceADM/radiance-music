<?php
// downloads.radianceph.com's gate (owner, 2026-11-08: "saten na mapupunta ang traffic hindi sa github", "mag apply pa
// den ng rate limiting, may exception tayo dahil tayo ay nag tetest"): every update file and latest.json is served
// from here with a limit per address, and none for our own testing (the X-Radiance-Tester key). The limits are
// generous: many Converge homes share one address. Over the limit: 429 with Retry-After (the apps then try GitHub).
// Resumed downloads (Range: bytes=N-) answer 206. Counters and keys live outside the web folder (~/downloads-gate).
declare(strict_types=1);

$home = '/home/' . get_current_user() . '/downloads-gate'; // (the hosting account's home, never served)
$limits = ['json' => 600, 'file' => 60]; // per address per hour (json: any; file: the same file)
if (is_file("$home/limits.json")) {
    $limits = array_merge($limits, json_decode((string) file_get_contents("$home/limits.json"), true) ?: []);
}

$f = (string) ($_GET['f'] ?? '');
$path = realpath(__DIR__ . '/' . $f);
$name = $path === false ? '' : basename($path);
if ($f === '' || $path === false || !str_starts_with($path, __DIR__ . '/') || !is_file($path) || $name[0] === '.' || str_ends_with($name, '.php')) {
    http_response_code(404);
    exit("Not here.\n");
}
$ext = strtolower(pathinfo($name, PATHINFO_EXTENSION));
$kind = $ext === 'json' ? 'json' : 'file';

// our own testing: no limit
$tester = false;
$given = (string) ($_SERVER['HTTP_X_RADIANCE_TESTER'] ?? '');
if ($given !== '' && is_file("$home/tester.keys")) {
    foreach (file("$home/tester.keys", FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: [] as $key) {
        if (hash_equals(trim($key), $given)) { $tester = true; break; }
    }
}

if (!$tester) {
    $ip = (string) ($_SERVER['REMOTE_ADDR'] ?? '0');
    $hour = gmdate('YmdH');
    $dir = "$home/count/$hour";
    if (!is_dir($dir)) {
        @mkdir($dir, 0700, true);
        // the hours before the last two: gone
        foreach (glob("$home/count/*", GLOB_ONLYDIR) ?: [] as $old) {
            if (basename($old) < gmdate('YmdH', time() - 7200)) {
                array_map('unlink', glob("$old/*") ?: []);
                @rmdir($old);
            }
        }
    }
    $counter = "$dir/" . sha1($ip . '|' . $kind . '|' . ($kind === 'file' ? $f : ''));
    $h = fopen($counter, 'c+');
    $n = 0;
    if ($h && flock($h, LOCK_EX)) {
        $n = (int) stream_get_contents($h) + 1;
        ftruncate($h, 0);
        rewind($h);
        fwrite($h, (string) $n);
        flock($h, LOCK_UN);
    }
    if ($h) fclose($h);
    // only first requests of a file count against the limit (a resumed download continues one)
    $resuming = isset($_SERVER['HTTP_RANGE']) && $kind === 'file';
    if ($n > $limits[$kind] && !$resuming) {
        http_response_code(429);
        header('Retry-After: ' . (3600 - time() % 3600));
        header('Content-Type: text/plain; charset=utf-8');
        exit("Too many requests from this address this hour. Try again later.\n");
    }
}

$types = ['json' => 'application/json', 'apk' => 'application/vnd.android.package-archive', 'exe' => 'application/octet-stream', 'flatpak' => 'application/octet-stream', 'gz' => 'application/gzip'];
header('Content-Type: ' . ($types[$ext] ?? 'application/octet-stream'));
header('Access-Control-Allow-Origin: *');
header('Cache-Control: ' . ($kind === 'json' ? 'no-cache, max-age=0' : 'public, max-age=31536000, immutable'));
header('Accept-Ranges: bytes');
if ($kind !== 'json') header('Content-Disposition: attachment; filename="' . $name . '"');

$size = filesize($path);
$from = 0;
if (isset($_SERVER['HTTP_RANGE']) && preg_match('/^bytes=(\d+)-$/', (string) $_SERVER['HTTP_RANGE'], $m)) {
    $from = (int) $m[1];
    if ($from >= $size) {
        http_response_code(416);
        header("Content-Range: bytes */$size");
        exit;
    }
    http_response_code(206);
    header("Content-Range: bytes $from-" . ($size - 1) . "/$size");
}
header('Content-Length: ' . ($size - $from));
if ($_SERVER['REQUEST_METHOD'] === 'HEAD') exit;

set_time_limit(0);
while (ob_get_level() > 0) ob_end_clean();
$in = fopen($path, 'rb');
fseek($in, $from);
while (!feof($in) && !connection_aborted()) {
    echo fread($in, 1 << 20);
    flush();
}
fclose($in);
