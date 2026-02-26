#pragma once

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

#include <QString>
#include <set>
#include <string>
#include <vector>

/// Lightweight ZIP-based mascot pack importer for platforms where
/// libshimejifinder is unavailable (e.g. MSVC builds on Windows).
///
/// Supports the common mascot pack layouts:
///   1. Root-level: actions.xml + behaviors.xml + img/
///   2. Shimeji-ee: shimeji-ee.jar + conf/ + img/<name>/
///   3. Subdirectory: <name>/conf/actions.xml + img/
///   4. Bare images: shime1.png .. shime46.png (uses built-in default XMLs)
class SimpleZipImporter {
public:
    /// Import mascot(s) from a ZIP archive at @p zipPath into @p mascotsDir.
    /// Returns the set of mascot names that were successfully imported.
    static std::set<std::string> import(QString const& zipPath,
                                        QString const& mascotsDir);

// Implementation details (public for internal static helper access)
    struct ZipEntry {
        std::string path;       // full path inside ZIP (forward-slash separated)
        std::string lowerName;  // lowercase filename component
        uint32_t index;         // index in mz_zip_archive
        bool isDir;
    };

    static std::string toLower(std::string const& s);
    static std::string lastComponent(std::string const& path);
    static std::string parentDir(std::string const& path);
    static std::string normalise(std::string const& path);

    /// Extract a single ZIP entry to an output file path.
    static bool extractEntry(void *zip, uint32_t index,
                             std::string const& outPath);

    /// Write raw data to an output file path, creating directories as needed.
    static bool writeFile(std::string const& outPath,
                          const char *data, size_t size);

    /// Try each known mascot pack layout; returns mascot names found.
    static std::set<std::string> tryImport(
        void *zip,
        std::vector<ZipEntry> const& entries,
        std::string const& defaultName,
        std::string const& mascotsDir);

    static std::set<std::string> tryRootLevel(
        void *zip, std::vector<ZipEntry> const& entries,
        std::string const& defaultName, std::string const& mascotsDir);

    static std::set<std::string> tryShimejiEE(
        void *zip, std::vector<ZipEntry> const& entries,
        std::string const& defaultName, std::string const& mascotsDir);

    static std::set<std::string> trySubdirectory(
        void *zip, std::vector<ZipEntry> const& entries,
        std::string const& defaultName, std::string const& mascotsDir);

    static std::set<std::string> tryBareImages(
        void *zip, std::vector<ZipEntry> const& entries,
        std::string const& defaultName, std::string const& mascotsDir);
};
