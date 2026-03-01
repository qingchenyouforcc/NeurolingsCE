//
// NeurolingsCE - Cross-platform shimeji desktop pet runner
// Copyright (C) 2025 pixelomer
// Copyright (C) 2026 qingchenyou
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//

#include "shijima-qt/SimpleZipImporter.hpp"
#include "miniz/miniz.h"

#include <algorithm>
#include <cctype>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <memory>

// ---------------------------------------------------------------------------
// Utility helpers
// ---------------------------------------------------------------------------

std::string SimpleZipImporter::toLower(std::string const& s) {
    std::string out = s;
    std::transform(out.begin(), out.end(), out.begin(),
                   [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
    return out;
}

std::string SimpleZipImporter::lastComponent(std::string const& path) {
    if (path.empty()) return {};
    auto pos = path.find_last_of('/');
    if (pos == std::string::npos) return path;
    return path.substr(pos + 1);
}

std::string SimpleZipImporter::parentDir(std::string const& path) {
    if (path.empty()) return {};
    auto pos = path.find_last_of('/');
    if (pos == std::string::npos) return {};
    return path.substr(0, pos);
}

std::string SimpleZipImporter::normalise(std::string const& path) {
    std::string out = path;
    // Convert backslashes to forward slashes
    std::replace(out.begin(), out.end(), '\\', '/');
    // Remove trailing slashes
    while (!out.empty() && out.back() == '/') {
        out.pop_back();
    }
    return out;
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

bool SimpleZipImporter::extractEntry(void *zip, uint32_t index,
                                     std::string const& outPath)
{
    auto *pZip = static_cast<mz_zip_archive *>(zip);

    // Ensure parent directory exists
    auto parent = std::filesystem::path(outPath).parent_path();
    std::error_code ec;
    std::filesystem::create_directories(parent, ec);
    if (ec) {
        std::cerr << "SimpleZipImporter: failed to create dir "
                  << parent.string() << ": " << ec.message() << std::endl;
        return false;
    }

    if (!mz_zip_reader_extract_to_file(pZip, index, outPath.c_str(), 0)) {
        std::cerr << "SimpleZipImporter: failed to extract index " << index
                  << " to " << outPath << std::endl;
        return false;
    }
    return true;
}

bool SimpleZipImporter::writeFile(std::string const& outPath,
                                  const char *data, size_t size)
{
    auto parent = std::filesystem::path(outPath).parent_path();
    std::error_code ec;
    std::filesystem::create_directories(parent, ec);
    if (ec) {
        std::cerr << "SimpleZipImporter: failed to create dir "
                  << parent.string() << ": " << ec.message() << std::endl;
        return false;
    }

    std::ofstream out(outPath, std::ios::binary);
    if (!out) {
        std::cerr << "SimpleZipImporter: failed to open " << outPath
                  << " for writing" << std::endl;
        return false;
    }
    out.write(data, static_cast<std::streamsize>(size));
    return out.good();
}

// ---------------------------------------------------------------------------
// Helper: find an entry by matching a directory prefix and a lower-case name
// ---------------------------------------------------------------------------

/// Return pointer to the first ZipEntry whose path starts with @p dirPrefix
/// and whose lowerName matches one of the candidate names.
static SimpleZipImporter::ZipEntry const*
findEntry(std::vector<SimpleZipImporter::ZipEntry> const& entries,
          std::string const& dirPrefix,
          std::initializer_list<std::string> const& candidateNames)
{
    for (auto &e : entries) {
        if (e.isDir) continue;
        // Check directory prefix (empty means root)
        std::string edir = SimpleZipImporter::parentDir(e.path);
        // Normalise comparison: if dirPrefix is empty, edir should be empty too
        if (dirPrefix.empty()) {
            if (!edir.empty()) continue;
        } else {
            if (edir != dirPrefix) continue;
        }
        for (auto &candidate : candidateNames) {
            if (e.lowerName == candidate) {
                return &e;
            }
        }
    }
    return nullptr;
}

/// Return all entries whose path starts with @p dirPrefix, have
/// the given extension (lower), and are not directories.
static std::vector<SimpleZipImporter::ZipEntry const*>
findAllWithExtension(std::vector<SimpleZipImporter::ZipEntry> const& entries,
                     std::string const& dirPrefix,
                     std::string const& lowerExt,
                     bool recursive = false)
{
    std::vector<SimpleZipImporter::ZipEntry const*> result;
    for (auto &e : entries) {
        if (e.isDir) continue;
        std::string edir = SimpleZipImporter::parentDir(e.path);
        if (recursive) {
            // Must start with dirPrefix (or equal)
            if (!dirPrefix.empty()) {
                if (edir != dirPrefix &&
                    edir.substr(0, dirPrefix.size() + 1) != dirPrefix + "/")
                    continue;
            }
        } else {
            if (dirPrefix.empty()) {
                if (!edir.empty()) continue;
            } else {
                if (edir != dirPrefix) continue;
            }
        }
        // Check extension
        auto dot = e.lowerName.rfind('.');
        if (dot == std::string::npos) continue;
        if (e.lowerName.substr(dot) == lowerExt) {
            // Skip icon.png (same as libshimejifinder)
            if (e.lowerName == "icon.png") continue;
            result.push_back(&e);
        }
    }
    return result;
}

/// List unique immediate subdirectory names under @p dirPrefix.
static std::vector<std::string>
listSubdirs(std::vector<SimpleZipImporter::ZipEntry> const& entries,
            std::string const& dirPrefix)
{
    std::set<std::string> seen;
    std::vector<std::string> result;
    std::string prefix = dirPrefix.empty() ? "" : dirPrefix + "/";
    for (auto &e : entries) {
        if (e.path.size() <= prefix.size()) continue;
        if (!prefix.empty() && e.path.substr(0, prefix.size()) != prefix) continue;
        auto rest = e.path.substr(prefix.size());
        auto slash = rest.find('/');
        if (slash == std::string::npos) continue; // file at this level, not subdir
        auto sub = rest.substr(0, slash);
        if (sub.empty()) continue;
        if (seen.insert(sub).second) {
            result.push_back(sub);
        }
    }
    return result;
}

/// Check if a given "directory" exists among entries (has at least one child).
static bool dirExists(std::vector<SimpleZipImporter::ZipEntry> const& entries,
                      std::string const& dirPath)
{
    std::string prefix = dirPath + "/";
    for (auto &e : entries) {
        if (e.path.size() > prefix.size() &&
            e.path.substr(0, prefix.size()) == prefix)
        {
            return true;
        }
    }
    return false;
}

// ---------------------------------------------------------------------------
// Extract helpers
// ---------------------------------------------------------------------------

/// Extract all .png images from dirPrefix into mascotsDir/<name>.mascot/img/
static int extractImages(void *zip,
                         std::vector<SimpleZipImporter::ZipEntry> const& entries,
                         std::string const& dirPrefix,
                         std::string const& name,
                         std::string const& mascotsDir)
{
    auto images = findAllWithExtension(entries, dirPrefix, ".png");
    int count = 0;
    for (auto *img : images) {
        std::string outPath = mascotsDir + "/" + name + ".mascot/img/" + img->lowerName;
        if (SimpleZipImporter::extractEntry(zip, img->index, outPath)) {
            ++count;
        }
    }
    return count;
}

/// Extract all .wav sounds from dirPrefix into mascotsDir/<name>.mascot/sound/
static int extractSounds(void *zip,
                         std::vector<SimpleZipImporter::ZipEntry> const& entries,
                         std::string const& dirPrefix,
                         std::string const& name,
                         std::string const& mascotsDir)
{
    auto sounds = findAllWithExtension(entries, dirPrefix, ".wav");
    int count = 0;
    for (auto *snd : sounds) {
        std::string outPath = mascotsDir + "/" + name + ".mascot/sound/" + snd->lowerName;
        if (SimpleZipImporter::extractEntry(zip, snd->index, outPath)) {
            ++count;
        }
    }
    return count;
}

/// Extract a single XML entry to mascotsDir/<name>.mascot/<xmlName>
static bool extractXml(void *zip, SimpleZipImporter::ZipEntry const* entry,
                       std::string const& name,
                       std::string const& xmlName,
                       std::string const& mascotsDir)
{
    if (entry == nullptr) return false;
    std::string outPath = mascotsDir + "/" + name + ".mascot/" + xmlName;
    return SimpleZipImporter::extractEntry(zip, entry->index, outPath);
}

// ---------------------------------------------------------------------------
// Format detection strategies
// ---------------------------------------------------------------------------

std::set<std::string> SimpleZipImporter::tryRootLevel(
    void *zip, std::vector<ZipEntry> const& entries,
    std::string const& defaultName, std::string const& mascotsDir)
{
    std::set<std::string> result;

    // Strategy 1a: root has actions.xml + behaviors.xml + img/ directly
    // (the format the user reported)
    auto actions = findEntry(entries, "", {"actions.xml", "action.xml",
                                           "\xe5\x8b\x95\xe4\xbd\x9c.xml"});
    auto behaviors = findEntry(entries, "", {"behaviors.xml", "behavior.xml",
                                             "\xe8\xa1\x8c\xe5\x8b\x95.xml"});

    if (actions != nullptr && behaviors != nullptr) {
        std::string name = defaultName;
        int imgCount = extractImages(zip, entries, "img", name, mascotsDir);
        if (imgCount < 2) {
            // Maybe images are at root level, not in img/
            imgCount = extractImages(zip, entries, "", name, mascotsDir);
        }
        if (imgCount >= 2) {
            extractXml(zip, actions, name, "actions.xml", mascotsDir);
            extractXml(zip, behaviors, name, "behaviors.xml", mascotsDir);
            // Also try sounds
            if (dirExists(entries, "sound")) {
                extractSounds(zip, entries, "sound", name, mascotsDir);
            }
            result.insert(name);
        }
    }

    // Strategy 1b: root has conf/actions.xml (standard shimeji format at root)
    if (result.empty()) {
        auto confActions = findEntry(entries, "conf",
            {"actions.xml", "action.xml", "\xe5\x8b\x95\xe4\xbd\x9c.xml"});
        auto confBehaviors = findEntry(entries, "conf",
            {"behaviors.xml", "behavior.xml", "\xe8\xa1\x8c\xe5\x8b\x95.xml"});
        if (confActions != nullptr && confBehaviors != nullptr) {
            std::string name = defaultName;
            // img/ at root or directly at root
            std::string imgDir = dirExists(entries, "img") ? "img" : "";
            int imgCount = extractImages(zip, entries, imgDir, name, mascotsDir);
            if (imgCount >= 2) {
                extractXml(zip, confActions, name, "actions.xml", mascotsDir);
                extractXml(zip, confBehaviors, name, "behaviors.xml", mascotsDir);
                if (dirExists(entries, "sound")) {
                    extractSounds(zip, entries, "sound", name, mascotsDir);
                }
                result.insert(name);
            }
        }
    }

    return result;
}

std::set<std::string> SimpleZipImporter::tryShimejiEE(
    void *zip, std::vector<ZipEntry> const& entries,
    std::string const& defaultName, std::string const& mascotsDir)
{
    std::set<std::string> result;

    // Look for shimeji-ee.jar at any level
    std::string jarDir;
    bool foundJar = false;
    for (auto &e : entries) {
        if (!e.isDir && e.lowerName == "shimeji-ee.jar") {
            jarDir = parentDir(e.path);
            foundJar = true;
            break;
        }
    }
    if (!foundJar) return result;

    std::string imgDir = jarDir.empty() ? "img" : jarDir + "/img";
    if (!dirExists(entries, imgDir)) {
        std::cerr << "SimpleZipImporter: shimeji-ee.jar found but no img/ folder"
                  << std::endl;
        return result;
    }

    // Find default conf XMLs
    std::string confDir = jarDir.empty() ? "conf" : jarDir + "/conf";
    auto defaultActions = findEntry(entries, confDir,
        {"actions.xml", "action.xml"});
    auto defaultBehaviors = findEntry(entries, confDir,
        {"behaviors.xml", "behavior.xml"});

    // Enumerate shimeji sub-folders under img/
    auto subfolders = listSubdirs(entries, imgDir);

    // Count valid shimeji for the Shimeji→defaultName rename logic
    int validCount = 0;
    for (auto &sub : subfolders) {
        if (toLower(sub) == "unused") continue;
        // Check if this subfolder has enough images
        std::string subImgDir = imgDir + "/" + sub;
        auto images = findAllWithExtension(entries, subImgDir, ".png");
        if (images.size() >= 2) {
            ++validCount;
        }
    }

    for (auto &sub : subfolders) {
        if (toLower(sub) == "unused") continue;

        std::string subImgDir = imgDir + "/" + sub;

        // Find per-shimeji conf
        std::string subConfDir = subImgDir + "/conf";
        auto actions = findEntry(entries, subConfDir,
            {"actions.xml", "action.xml"});
        auto behaviors = findEntry(entries, subConfDir,
            {"behaviors.xml", "behavior.xml"});
        if (actions == nullptr) actions = defaultActions;
        if (behaviors == nullptr) behaviors = defaultBehaviors;
        if (actions == nullptr || behaviors == nullptr) continue;

        std::string name = (sub == "Shimeji" && validCount == 1)
                            ? defaultName : sub;

        int imgCount = extractImages(zip, entries, subImgDir, name, mascotsDir);
        if (imgCount >= 2) {
            extractXml(zip, actions, name, "actions.xml", mascotsDir);
            extractXml(zip, behaviors, name, "behaviors.xml", mascotsDir);

            // Try sound
            std::string subSoundDir = subImgDir + "/sound";
            if (dirExists(entries, subSoundDir)) {
                extractSounds(zip, entries, subSoundDir, name, mascotsDir);
            } else {
                // Fallback to root-level sound dir
                std::string rootSoundDir = jarDir.empty() ? "sound"
                    : jarDir + "/sound";
                if (dirExists(entries, rootSoundDir)) {
                    extractSounds(zip, entries, rootSoundDir, name, mascotsDir);
                }
            }
            result.insert(name);
        }
    }

    return result;
}

std::set<std::string> SimpleZipImporter::trySubdirectory(
    void *zip, std::vector<ZipEntry> const& entries,
    std::string const& defaultName, std::string const& mascotsDir)
{
    std::set<std::string> result;

    // Look for actions.xml inside a conf/ subdirectory anywhere in the archive.
    // This handles:  <name>/conf/actions.xml  or  <name>/shimeji.jar etc.
    for (auto &e : entries) {
        if (e.isDir) continue;
        auto lower = toLower(e.lowerName);
        if (lower != "actions.xml" && lower != "action.xml" &&
            lower != "\xe5\x8b\x95\xe4\xbd\x9c.xml")
            continue;

        std::string dir = parentDir(e.path);
        std::string dirName = lastComponent(dir);
        if (toLower(dirName) != "conf") continue;

        // The shimeji root is the parent of conf/
        std::string shimejiRoot = parentDir(dir);
        if (shimejiRoot.empty()) continue; // already handled by tryRootLevel

        // Already imported?
        std::string name = lastComponent(shimejiRoot);
        if (name.empty() || name == "Shimeji") name = defaultName;
        if (result.count(name)) continue;

        // Find behaviors
        auto behaviors = findEntry(entries, dir,
            {"behaviors.xml", "behavior.xml", "\xe8\xa1\x8c\xe5\x8b\x95.xml"});
        auto actions = &e;
        if (behaviors == nullptr) continue;

        // Images: try <root>/img/ first, then <root>/ itself
        std::string imgDir = shimejiRoot + "/img";
        int imgCount = extractImages(zip, entries, imgDir, name, mascotsDir);
        if (imgCount < 2) {
            imgCount = extractImages(zip, entries, shimejiRoot, name, mascotsDir);
        }
        if (imgCount >= 2) {
            extractXml(zip, actions, name, "actions.xml", mascotsDir);
            extractXml(zip, behaviors, name, "behaviors.xml", mascotsDir);

            std::string soundDir = shimejiRoot + "/sound";
            if (dirExists(entries, soundDir)) {
                extractSounds(zip, entries, soundDir, name, mascotsDir);
            }
            result.insert(name);
        }
    }

    return result;
}

std::set<std::string> SimpleZipImporter::tryBareImages(
    void *zip, std::vector<ZipEntry> const& entries,
    std::string const& defaultName, std::string const& mascotsDir)
{
    std::set<std::string> result;

    // Look for shime1.png at any directory level
    for (auto &e : entries) {
        if (e.isDir) continue;
        if (e.lowerName != "shime1.png") continue;

        std::string dir = parentDir(e.path);
        std::string name = dir.empty() ? defaultName : lastComponent(dir);
        if (name.empty() || name == "Shimeji") name = defaultName;
        if (result.count(name)) continue;

        // Verify shime1..shime46.png all exist, and shime47.png does NOT
        bool allPresent = true;
        std::map<int, ZipEntry const*> shimeEntries;
        for (auto &candidate : entries) {
            if (candidate.isDir) continue;
            if (parentDir(candidate.path) != dir) continue;
            // Check if it's shimeN.png
            auto lower = toLower(candidate.lowerName);
            if (lower.size() > 5 && lower.substr(0, 5) == "shime" &&
                lower.substr(lower.size() - 4) == ".png")
            {
                auto numStr = lower.substr(5, lower.size() - 9);
                try {
                    int num = std::stoi(numStr);
                    shimeEntries[num] = &candidate;
                } catch (...) {}
            }
        }

        // shime47 must NOT exist (same check as libshimejifinder)
        if (shimeEntries.count(47)) continue;

        for (int i = 1; i <= 46; ++i) {
            if (!shimeEntries.count(i)) {
                allPresent = false;
                break;
            }
        }
        if (!allPresent) continue;

        // Extract all 46 images
        std::string outBase = mascotsDir + "/" + name + ".mascot/img/";
        int count = 0;
        for (int i = 1; i <= 46; ++i) {
            auto *entry = shimeEntries[i];
            std::string outPath = outBase + entry->lowerName;
            if (extractEntry(zip, entry->index, outPath)) {
                ++count;
            }
        }

        if (count >= 2) {
            // Write default XMLs — we use the bundled DefaultMascot XMLs
            // which reference shime1-46.png
            // Since we don't have them embedded here, we write minimal
            // placeholder XMLs that the engine can parse.
            // The DefaultMascot XMLs are compiled into the binary and loaded
            // separately; for bare-image imports we need standalone XMLs.
            // We embed minimal versions inline.
            static const char *minimalActions =
                "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n"
                "<Mascot xmlns=\"http://www.group-finity.com/Mascot\">\n"
                "  <ActionList>\n"
                "    <Action Name=\"Look\" Type=\"Embedded\" Class=\"com.group_finity.mascot.action.Look\" />\n"
                "    <Action Name=\"Offset\" Type=\"Embedded\" Class=\"com.group_finity.mascot.action.Offset\" />\n"
                "    <Action Name=\"Stand\" Type=\"Stay\" BorderType=\"Floor\">\n"
                "      <Animation>\n"
                "        <Pose Image=\"/shime1.png\" ImageAnchor=\"64,128\" Velocity=\"0,0\" Duration=\"250\" />\n"
                "      </Animation>\n"
                "    </Action>\n"
                "    <Action Name=\"Walk\" Type=\"Move\" BorderType=\"Floor\">\n"
                "      <Animation>\n"
                "        <Pose Image=\"/shime1.png\" ImageAnchor=\"64,128\" Velocity=\"-2,0\" Duration=\"6\" />\n"
                "        <Pose Image=\"/shime2.png\" ImageAnchor=\"64,128\" Velocity=\"-2,0\" Duration=\"6\" />\n"
                "        <Pose Image=\"/shime1.png\" ImageAnchor=\"64,128\" Velocity=\"-2,0\" Duration=\"6\" />\n"
                "        <Pose Image=\"/shime3.png\" ImageAnchor=\"64,128\" Velocity=\"-2,0\" Duration=\"6\" />\n"
                "      </Animation>\n"
                "    </Action>\n"
                "    <Action Name=\"Fall\" Type=\"Move\" BorderType=\"\" Gravity=\"20\">\n"
                "      <Animation>\n"
                "        <Pose Image=\"/shime4.png\" ImageAnchor=\"64,128\" Velocity=\"0,0\" Duration=\"200\" />\n"
                "      </Animation>\n"
                "    </Action>\n"
                "    <Action Name=\"Dragged\" Type=\"Move\" Draggable=\"true\">\n"
                "      <Animation>\n"
                "        <Pose Image=\"/shime5.png\" ImageAnchor=\"64,128\" Velocity=\"0,0\" Duration=\"100\" />\n"
                "      </Animation>\n"
                "    </Action>\n"
                "    <Action Name=\"Thrown\" Type=\"Move\" BorderType=\"\" Gravity=\"20\">\n"
                "      <Animation>\n"
                "        <Pose Image=\"/shime4.png\" ImageAnchor=\"64,128\" Velocity=\"0,0\" Duration=\"200\" />\n"
                "      </Animation>\n"
                "    </Action>\n"
                "  </ActionList>\n"
                "</Mascot>\n";

            static const char *minimalBehaviors =
                "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n"
                "<Mascot xmlns=\"http://www.group-finity.com/Mascot\">\n"
                "  <BehaviorList>\n"
                "    <Behavior Name=\"ChaseMouse\" Frequency=\"0\" Hidden=\"true\">\n"
                "      <NextBehavior Add=\"false\">\n"
                "        <BehaviorReference Name=\"Stand\" Frequency=\"1\" />\n"
                "      </NextBehavior>\n"
                "    </Behavior>\n"
                "    <Behavior Name=\"Fall\" Frequency=\"0\" Hidden=\"true\" />\n"
                "    <Behavior Name=\"Dragged\" Frequency=\"0\" Hidden=\"true\" />\n"
                "    <Behavior Name=\"Thrown\" Frequency=\"0\" Hidden=\"true\" />\n"
                "    <Behavior Name=\"Stand\" Frequency=\"10\" />\n"
                "    <Behavior Name=\"Walk\" Frequency=\"10\" />\n"
                "  </BehaviorList>\n"
                "</Mascot>\n";

            std::string xmlBase = mascotsDir + "/" + name + ".mascot/";
            writeFile(xmlBase + "actions.xml", minimalActions, std::strlen(minimalActions));
            writeFile(xmlBase + "behaviors.xml", minimalBehaviors, std::strlen(minimalBehaviors));
            result.insert(name);
        }
    }

    return result;
}

// ---------------------------------------------------------------------------
// Main import orchestration
// ---------------------------------------------------------------------------

std::set<std::string> SimpleZipImporter::tryImport(
    void *zip,
    std::vector<ZipEntry> const& entries,
    std::string const& defaultName,
    std::string const& mascotsDir)
{
    // Try strategies in the same order as libshimejifinder/analyze.cc:
    // 1. Shimeji-EE (shimeji-ee.jar)
    auto result = tryShimejiEE(zip, entries, defaultName, mascotsDir);
    if (!result.empty()) return result;

    // 2. Root-level (actions.xml at root or conf/ at root)
    result = tryRootLevel(zip, entries, defaultName, mascotsDir);
    if (!result.empty()) return result;

    // 3. Subdirectory (conf/actions.xml inside a named folder)
    result = trySubdirectory(zip, entries, defaultName, mascotsDir);
    if (!result.empty()) return result;

    // 4. Bare images (shime1.png .. shime46.png)
    result = tryBareImages(zip, entries, defaultName, mascotsDir);
    return result;
}

std::set<std::string> SimpleZipImporter::import(QString const& zipPath,
                                                 QString const& mascotsDir)
{
    std::string zipPathStr = zipPath.toStdString();
    std::string mascotsDirStr = mascotsDir.toStdString();

    // Normalise mascotsDir path separators
    std::replace(mascotsDirStr.begin(), mascotsDirStr.end(), '\\', '/');

    // Derive default name from the ZIP filename (without extension)
    std::string defaultName;
    {
        auto p = std::filesystem::path(zipPathStr);
        defaultName = p.stem().string();
        if (defaultName.empty()) {
            defaultName = "Imported";
        }
    }

    mz_zip_archive zipArchive;
    std::memset(&zipArchive, 0, sizeof(zipArchive));

    if (!mz_zip_reader_init_file(&zipArchive, zipPathStr.c_str(), 0)) {
        std::cerr << "SimpleZipImporter: failed to open ZIP: " << zipPathStr
                  << std::endl;
        return {};
    }

    // Build entry list
    std::vector<ZipEntry> entries;
    mz_uint numFiles = mz_zip_reader_get_num_files(&zipArchive);
    entries.reserve(numFiles);

    for (mz_uint i = 0; i < numFiles; ++i) {
        mz_zip_archive_file_stat stat;
        if (!mz_zip_reader_file_stat(&zipArchive, i, &stat)) {
            continue;
        }
        ZipEntry e;
        e.path = normalise(stat.m_filename);
        e.index = i;
        e.isDir = mz_zip_reader_is_file_a_directory(&zipArchive, i) != 0;

        // Extract lowercase filename
        e.lowerName = toLower(lastComponent(e.path));

        entries.push_back(std::move(e));
    }

    auto result = tryImport(&zipArchive, entries, defaultName, mascotsDirStr);

    mz_zip_reader_end(&zipArchive);

    if (result.empty()) {
        std::cerr << "SimpleZipImporter: no mascots found in " << zipPathStr
                  << std::endl;
    } else {
        for (auto &name : result) {
            std::cout << "SimpleZipImporter: imported mascot: " << name
                      << std::endl;
        }
    }

    return result;
}
