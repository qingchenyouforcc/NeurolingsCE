//! 桌宠音效：行为切换帧上声明的 Sound 属性驱动，wav 文件来自桌宠包
//! 的 sound 目录。播放是尽力而为的：无声卡或文件损坏时静默跳过。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rodio::{Decoder, Player};

/// 单个音效文件的大小上限（16 MiB）。
const AUDIO_FILE_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// 进程级共享音频输出流；初始化失败则全局静音。
struct AudioOut {
    _sink: rodio::MixerDeviceSink,
    mixer: rodio::mixer::Mixer,
}

fn audio_out() -> Option<&'static AudioOut> {
    static OUT: OnceLock<Option<AudioOut>> = OnceLock::new();
    OUT.get_or_init(|| {
        rodio::DeviceSinkBuilder::open_default_sink()
            .ok()
            .map(|sink| AudioOut {
                mixer: sink.mixer().clone(),
                _sink: sink,
            })
    })
    .as_ref()
}

/// 一只桌宠的音效播放器。
pub struct SoundPlayer {
    sound_dir: PathBuf,
    /// 已校验可用的音效文件缓存。
    resolved: HashMap<String, Option<PathBuf>>,
    active: Option<Player>,
}

impl SoundPlayer {
    pub fn new(sound_dir: &Path) -> Option<Self> {
        audio_out()?;
        Some(Self {
            sound_dir: sound_dir.to_path_buf(),
            resolved: HashMap::new(),
            active: None,
        })
    }

    fn resolve(&mut self, name: &str) -> Option<PathBuf> {
        if let Some(cached) = self.resolved.get(name) {
            return cached.clone();
        }
        let found = resolve_sound_path(&self.sound_dir, name);
        self.resolved.insert(name.to_string(), found.clone());
        found
    }

    /// 切换到指定音效；空名称表示停止。
    pub fn play(&mut self, name: &str) {
        self.stop();
        let Some(path) = self.resolve(name) else {
            return;
        };
        let Some(out) = audio_out() else { return };
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => return,
        };
        let decoder = match Decoder::new(std::io::BufReader::new(file)) {
            Ok(decoder) => decoder,
            Err(_) => return,
        };
        let player = Player::connect_new(&out.mixer);
        player.append(decoder);
        self.active = Some(player);
    }

    pub fn stop(&mut self) {
        if let Some(player) = self.active.take() {
            player.stop();
        }
    }

    /// 当前是否有音效在播。
    pub fn playing(&self) -> bool {
        self.active.as_ref().is_some_and(|p| !p.empty())
    }
}

/// 在音效根目录中解析符合路径安全约束的相对子路径。
fn resolve_sound_path(sound_dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = neurolings_pack::safe_child_path(sound_dir, name)?;
    let metadata = std::fs::metadata(&candidate).ok()?;
    (metadata.is_file() && metadata.len() <= AUDIO_FILE_MAX_BYTES).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_nested_sound_path() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("voice");
        std::fs::create_dir(&nested).unwrap();
        let sound = nested.join("hit.wav");
        std::fs::write(&sound, b"test").unwrap();

        assert_eq!(
            resolve_sound_path(root.path(), "voice/hit.wav"),
            Some(sound)
        );
    }

    #[test]
    fn rejects_unsafe_sound_path() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(resolve_sound_path(root.path(), "../hit.wav"), None);
        assert_eq!(resolve_sound_path(root.path(), "/hit.wav"), None);
    }
}
