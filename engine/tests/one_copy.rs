//! One copy at a time (issue #45): a second copy asks the running one to show its window (and to
//! open its link), today's GTK app and the Windows beta alike. A data folder of its own.

use std::sync::mpsc::channel;
use std::time::Duration;

use radiance_engine::one_copy;

#[test]
fn the_running_copy_is_asked_to_show() {
    let base = std::env::temp_dir().join(format!("radiance-one-copy-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::env::set_var("XDG_DATA_HOME", &base);
    assert!(!one_copy::ask_the_other(None), "nobody's running yet");
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    one_copy::listen(move |link| {
        let _ = tx.lock().unwrap().send(link);
    });
    assert!(one_copy::ask_the_other(None));
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), None);
    let link = "radiancemusic://join?u=https%3A%2F%2Fmusic.example.com%2Fjoin%2FABCD";
    assert!(one_copy::ask_the_other(Some(link)));
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap().as_deref(), Some(link));
    let _ = std::fs::remove_dir_all(base);
}
