// 
// Shijima-Qt - Cross-platform shimeji simulation app for desktop
// Copyright (C) 2025 pixelomer
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

#include <QApplication>
#include <QIcon>
#include <QDir>
#include <QStandardPaths>
#include <QMessageBox>
#include <shijima/log.hpp>
#include "Platform/Platform.hpp"
#include "shijima-qt/ShijimaManager.hpp"
#include "shijima-qt/AssetLoader.hpp"
#include "shijima-qt/cli.hpp"
#include <httplib.h>

int main(int argc, char **argv) {
    if (argc > 1) {
        return shijimaRunCli(argc, argv);
    }
    Platform::initialize(argc, argv);
    #ifdef SHIJIMA_LOGGING_ENABLED
        shijima::set_log_level(SHIJIMA_LOG_PARSER | SHIJIMA_LOG_WARNINGS);
    #endif
    QApplication app(argc, argv);
    app.setApplicationName("Shijima-Qt");
    app.setApplicationDisplayName("Shijima-Qt");
    {
        QIcon appIcon { QStringLiteral(":/shijima-qt.ico") };
        if (appIcon.isNull()) {
            appIcon = QIcon { QStringLiteral(":/shijima-qt.png") };
        }
        if (!appIcon.isNull()) {
            app.setWindowIcon(appIcon);
        }
    }
    try {
        httplib::Client pingClient { "http://127.0.0.1:32456" };
        pingClient.set_connection_timeout(0, 500000);
        pingClient.set_read_timeout(0, 500000);
        auto pingResult = pingClient.Get("/shijima/api/v1/ping");
        if (pingResult != nullptr) {
            throw std::runtime_error(QCoreApplication::translate("main", "Shijima-Qt is already running!").toStdString());
        }
        ShijimaManager::defaultManager()->show();
    }
    catch (std::exception &ex) {
        QMessageBox *msg = new QMessageBox {};
        msg->setText(QCoreApplication::translate("main", "Shijima-Qt failed to start. Reason: ") +
            QString::fromUtf8(ex.what()));
        msg->setStandardButtons(QMessageBox::StandardButton::Close);
        msg->setAttribute(Qt::WA_DeleteOnClose);
        msg->show();
    }
    int ret = app.exec();
    ShijimaManager::finalize();
    AssetLoader::finalize();
    return ret;
}
