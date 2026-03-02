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

#include "shijima-qt/ShijimaManager.hpp"
#include <cmath>
#include <exception>
#include <filesystem>
#include <iostream>
#include <QVariant>
#include <QVBoxLayout>
#include <QWidget>
#include <QCloseEvent>
#include <QMenuBar>
#include <QFileDialog>
#include <QPushButton>
#include <QWindow>
#include <QTextStream>
#include <QGuiApplication>
#include <QFile>
#include <QDesktopServices>
#include <QScreen>
#include <QRandomGenerator>
#include "shijima-qt/PlatformWidget.hpp"
#include "shijima-qt/ShijimaLicensesDialog.hpp"
#include "shijima-qt/ShijimaWidget.hpp"
#include <QDirIterator>
#include <QDesktopServices>
#include <shijima/mascot/factory.hpp>
#if SHIJIMA_WITH_SHIMEJIFINDER
#include <shimejifinder/analyze.hpp>
#endif
#if !SHIJIMA_WITH_SHIMEJIFINDER
#include "shijima-qt/SimpleZipImporter.hpp"
#endif
#include <QStandardPaths>
#include "shijima-qt/ForcedProgressDialog.hpp"
#include <QAbstractItemModel>
#include <QAction>
#include <QActionGroup>
#include <QCoreApplication>
#include <QCursor>
#include <QDesktopServices>
#include <QFileDialog>
#include <QItemSelectionModel>
#include <QKeySequence>
#include <QListWidget>
#include <QMessageBox>
#include <QProcess>
#include <QUrl>
#include <QtConcurrent>
#include <string>
#include <QLabel>
#include <QFormLayout>
#include <QColorDialog>
#include <cstring>
#include <cstdint>
#include <QSystemTrayIcon>
#include <QMenu>
#include <QStyle>
#include <QDoubleSpinBox>
#include <QSpinBox>
#include <QHBoxLayout>
#include <QSignalBlocker>

#include <QCheckBox>
#include <QRadioButton>
#include <QDialogButtonBox>
#include "ElaScrollPageArea.h"
#include "ElaText.h"
#include "ElaToggleSwitch.h"
#include <QSlider>
#include "ElaStatusBar.h"
#include "ElaTheme.h"
#include <QStyleHints>
#include "ElaPushButton.h"
#define SHIJIMAQT_SUBTICK_COUNT 4

#ifndef NEUROLINGSCE_VERSION
#define NEUROLINGSCE_VERSION "0.1.0"
#endif

using namespace shijima;

static QString colorToString(QColor const& color) {
    auto rgb = color.toRgb();
    std::array<char, 8> buf;
    snprintf(&buf[0], buf.size(), "#%02hhX%02hhX%02hhX",
        (uint8_t)rgb.red(), (uint8_t)rgb.green(),
        (uint8_t)rgb.blue());
    buf[buf.size()-1] = 0;
    return QString { &buf[0] };
}

// https://stackoverflow.com/questions/34135624/-/54029758#54029758
static void dispatchToMainThread(std::function<void()> callback) {
    QTimer *timer = new QTimer;
    timer->moveToThread(qApp->thread());
    timer->setSingleShot(true);
    QObject::connect(timer, &QTimer::timeout, [timer, callback]() {
        callback();
        timer->deleteLater();
    });
    QMetaObject::invokeMethod(timer, "start", Qt::QueuedConnection, Q_ARG(int, 0));
}


static ShijimaManager *m_defaultManager = nullptr;

static QIcon makeTrayIconFallback(QWidget *w) {
    // Prefer the icon embedded via Qt resource system (resources.qrc).
    {
        QIcon ico { QStringLiteral(":/neurolingsce.ico") };
        if (!ico.isNull()) {
            return ico;
        }
    }

    // Fallback: try the PNG resource.
    {
        QIcon ico { QStringLiteral(":/neurolingsce.png") };
        if (!ico.isNull()) {
            return ico;
        }
    }

    if (qApp != nullptr) {
        QIcon appIco = qApp->windowIcon();
        if (!appIco.isNull()) {
            return appIco;
        }
    }

    if (w != nullptr) {
        QIcon winIco = w->windowIcon();
        if (!winIco.isNull()) {
            return winIco;
        }
    }

    QIcon themed = QIcon::fromTheme("shijima-qt");
    if (!themed.isNull()) {
        return themed;
    }
    if (qApp != nullptr && qApp->style() != nullptr) {
        return qApp->style()->standardIcon(QStyle::SP_ComputerIcon);
    }
    return {};
}

static QSystemTrayIcon *g_trayIcon = nullptr;
static QMenu *g_trayMenu = nullptr;

static void rebuildTrayMenuFor(ShijimaManager *self) {
    if (g_trayMenu == nullptr || self == nullptr) {
        return;
    }

    g_trayMenu->clear();

    QAction *toggleAction = g_trayMenu->addAction(self->isVisible() ? QCoreApplication::translate("ShijimaManager", "Hide") : QCoreApplication::translate("ShijimaManager", "Show"));
    QObject::connect(toggleAction, &QAction::triggered, [self]() {
        self->setManagerVisible(!self->isVisible());
        rebuildTrayMenuFor(self);
    });

    QMenu *spawnMenu = g_trayMenu->addMenu(QCoreApplication::translate("ShijimaManager", "Spawn"));
    auto names = self->loadedMascots().keys();
    names.sort(Qt::CaseInsensitive);
    for (auto const& name : names) {
        QAction *a = spawnMenu->addAction(name);
        QObject::connect(a, &QAction::triggered, [self, name]() {
            self->spawn(name.toStdString());
        });
    }
    if (names.isEmpty()) {
        QAction *empty = spawnMenu->addAction(QCoreApplication::translate("ShijimaManager", "(none)"));
        empty->setEnabled(false);
    }

    g_trayMenu->addSeparator();

    QAction *killAllAction = g_trayMenu->addAction(QCoreApplication::translate("ShijimaManager", "Kill all"));
    QObject::connect(killAllAction, &QAction::triggered, [self]() {
        self->killAll();
    });

    QAction *quitAction = g_trayMenu->addAction(QCoreApplication::translate("ShijimaManager", "Quit"));
    QObject::connect(quitAction, &QAction::triggered, [self]() {
        QMetaObject::invokeMethod(self, [self]() {
            self->quitAction();
        }, Qt::QueuedConnection);
    });
}

static void setupTrayIconFor(ShijimaManager *self) {
    if (self == nullptr) {
        return;
    }
    if (!QSystemTrayIcon::isSystemTrayAvailable()) {
        return;
    }
    if (g_trayIcon != nullptr) {
        return;
    }

    g_trayIcon = new QSystemTrayIcon(self);
    g_trayIcon->setToolTip(QCoreApplication::translate("ShijimaManager", "Shijima-Qt"));
    g_trayIcon->setIcon(makeTrayIconFallback(self));

    g_trayMenu = new QMenu(self);
    g_trayIcon->setContextMenu(g_trayMenu);
    rebuildTrayMenuFor(self);

    QObject::connect(g_trayIcon, &QSystemTrayIcon::activated,
        [self](QSystemTrayIcon::ActivationReason reason) {
            if (reason == QSystemTrayIcon::Trigger || reason == QSystemTrayIcon::DoubleClick) {
                self->setManagerVisible(!self->isVisible());
                rebuildTrayMenuFor(self);
            }
        });

    g_trayIcon->show();
}

ShijimaManager *ShijimaManager::defaultManager() {
    if (m_defaultManager == nullptr) {
        m_defaultManager = new ShijimaManager;
    }
    return m_defaultManager;
}

void ShijimaManager::finalize() {
    if (m_defaultManager != nullptr) {
        delete m_defaultManager;
        m_defaultManager = nullptr;
    }
}

// tray icon helpers are file-local (see above)

void ShijimaManager::killAll() {
    for (auto mascot : m_mascots) {
        mascot->markForDeletion();
    }
}

void ShijimaManager::killAll(QString const& name) {
    for (auto mascot : m_mascots) {
        if (mascot->mascotName() == name) {
            mascot->markForDeletion();
        }
    }
}

void ShijimaManager::killAllButOne(ShijimaWidget *widget) {
    for (auto mascot : m_mascots) {
        if (widget == mascot) {
            continue;
        }
        mascot->markForDeletion();
    }
}

void ShijimaManager::killAllButOne(QString const& name) {
    bool foundOne = false;
    for (auto mascot : m_mascots) {
        if (mascot->mascotName() == name) {
            if (!foundOne) {
                foundOne = true;
                continue;
            }
            mascot->markForDeletion();
        }
    }
}

void ShijimaManager::loadData(MascotData *data) {
    if (data != nullptr && data->valid()) {
        shijima::mascot::factory::tmpl tmpl;
        tmpl.actions_xml = data->actionsXML().toStdString();
        tmpl.behaviors_xml = data->behaviorsXML().toStdString();
        tmpl.name = data->name().toStdString();
        tmpl.path = data->path().toStdString();
        m_factory.register_template(tmpl);
        m_loadedMascots.insert(data->name(), data);
        m_loadedMascotsById.insert(data->id(), data);
        std::cout << "Loaded mascot: " << data->name().toStdString() << std::endl;
    }
    else {
        throw std::runtime_error("loadData() called with invalid data");
    }
}

void ShijimaManager::loadDefaultMascot() {
    auto data = new MascotData { "@", m_idCounter++ };
    loadData(data);
}

QMap<QString, MascotData *> const& ShijimaManager::loadedMascots() {
    return m_loadedMascots;
}

QMap<int, MascotData *> const& ShijimaManager::loadedMascotsById() {
    return m_loadedMascotsById;
}

std::list<ShijimaWidget *> const& ShijimaManager::mascots() {
    return m_mascots;
}

std::map<int, ShijimaWidget *> const& ShijimaManager::mascotsById() {
    return m_mascotsById;
}


void ShijimaManager::reloadMascot(QString const& name) {
    if (m_loadedMascots.contains(name) && !m_loadedMascots[name]->deletable()) {
        std::cout << "Refusing to unload mascot: " << name.toStdString()
            << std::endl;
        return;
    }
    MascotData *data = nullptr;
    try {
        data = new MascotData { m_mascotsPath + QDir::separator() + name + ".mascot",
            m_idCounter++ };
    }
    catch (std::exception &ex) {
        std::cerr << "couldn't load mascot: " << name.toStdString() << std::endl;
        std::cerr << ex.what() << std::endl;
    }
    if (m_loadedMascots.contains(name)) {
        MascotData *data = m_loadedMascots[name];
        m_factory.deregister_template(name.toStdString());
        data->unloadCache();
        killAll(name);
        m_loadedMascots.remove(name);
        m_loadedMascotsById.remove(data->id());
        delete data;
        std::cout << "Unloaded mascot: " << name.toStdString() << std::endl;
    }
    if (data != nullptr) {
        if (data->name() != name) {
            throw std::runtime_error("Impossible condition: New mascot name is incorrect");
        }
        loadData(data);
    }
    m_listItemsToRefresh.insert(name);

    rebuildTrayMenuFor(this);
}

void ShijimaManager::importAction() {
    auto paths = QFileDialog::getOpenFileNames(this, tr("Choose shimeji archive..."));
    if (paths.isEmpty()) {
        return;
    }
    importWithDialog(paths);
}

void ShijimaManager::quitAction() {
    m_allowClose = true;
    closeWindow();
}

void ShijimaManager::deleteAction() {
    if (m_loadedMascots.size() == 0) {
        return;
    }
    auto selected = m_listWidget.selectedItems();
    for (long i=(long)selected.size()-1; i>=0; --i) {
        auto mascotData = m_loadedMascots[selected[i]->text()];
        if (!mascotData->deletable()) {
            selected.remove(i);
        }
    }
    if (selected.size() == 0) {
        return;
    }
    QString msg = tr("Are you sure you want to delete these shimeji?");
    for (long i=0; i<selected.size() && i<5; ++i) {
        msg += "\n* " + selected[i]->text();
    }
    if (selected.size() > 5) {
        msg += tr("\n... and %1 other(s)").arg(selected.size() - 5);
    }
    QMessageBox msgBox { this };
    msgBox.setWindowTitle(tr("Delete shimeji"));
    msgBox.setText(msg);
    msgBox.setStandardButtons(QMessageBox::StandardButton::Yes |
        QMessageBox::StandardButton::No);
    msgBox.setIcon(QMessageBox::Icon::Question);
    int ret = msgBox.exec();
    if (ret == QMessageBox::StandardButton::Yes) {
        for (auto item : selected) {
            auto mascotData = m_loadedMascots[item->text()];
            if (!mascotData->deletable()) {
                continue;
            }
            std::filesystem::path path = mascotData->path().toStdString();
            std::cout << "Deleting mascot: " << item->text().toStdString() << std::endl;
            try {
                // remove_all(path) could be dangerous
                std::filesystem::remove_all(path / "img");
                std::filesystem::remove_all(path / "sound");
                std::filesystem::remove(path / "actions.xml");
                std::filesystem::remove(path / "behaviors.xml");
                std::filesystem::remove(path);
            }
            catch (std::exception &ex) {
                std::cerr << "failed to delete mascot: " << path.string()
                    << ": " << ex.what() << std::endl;
            }
            reloadMascot(item->text());
        }
        refreshListWidget();
    }
}

std::unique_lock<std::mutex> ShijimaManager::acquireLock() {
    return std::unique_lock<std::mutex> { m_mutex };
}

void ShijimaManager::updateSandboxBackground() {
    if (m_sandboxWidget != nullptr) {
        m_sandboxWidget->setStyleSheet("#sandboxWindow { background-color: " +
            colorToString(m_sandboxBackground) + "; }");
    }
}

void ShijimaManager::setupNavigation() {
    // ==================== HOME PAGE ====================
    m_homePage = new QWidget(this);
    auto *homeLayout = new QVBoxLayout(m_homePage);
    homeLayout->setContentsMargins(10, 10, 10, 10);
    homeLayout->setSpacing(8);

    // Top action button row
    auto *actionRow = new QHBoxLayout;
    actionRow->setSpacing(8);

    auto *btnSpawn = new ElaPushButton(tr("Spawn Random"));
    connect(btnSpawn, &ElaPushButton::clicked, this, &ShijimaManager::spawnClicked);
    actionRow->addWidget(btnSpawn);

    auto *btnImport = new ElaPushButton(tr("Import"));
    connect(btnImport, &ElaPushButton::clicked, this, &ShijimaManager::importAction);
    actionRow->addWidget(btnImport);

    auto *btnDelete = new ElaPushButton(tr("Delete"));
    connect(btnDelete, &ElaPushButton::clicked, this, &ShijimaManager::deleteAction);
    actionRow->addWidget(btnDelete);

    auto *btnFolder = new ElaPushButton(tr("Show Folder"));
    connect(btnFolder, &ElaPushButton::clicked, [this](){
        QDesktopServices::openUrl(QUrl::fromLocalFile(m_mascotsPath));
    });
    actionRow->addWidget(btnFolder);

    actionRow->addStretch();
    homeLayout->addLayout(actionRow);

    // Reparent listWidget into home page
    m_listWidget.setParent(m_homePage);
    homeLayout->addWidget(&m_listWidget, 1);

    addPageNode(tr("Home"), m_homePage, ElaIconType::House);

    // ==================== SETTINGS PAGE ====================
    m_settingsPage = new QWidget(this);
    auto *settingsLayout = new QVBoxLayout(m_settingsPage);
    settingsLayout->setContentsMargins(10, 10, 10, 10);
    settingsLayout->setSpacing(12);

    // --- Multiplication ---
    {
        static const QString key = "multiplicationEnabled";
        bool initial = m_settings.value(key, QVariant::fromValue(true)).toBool();
        for (auto &env : m_env) env->allows_breeding = initial;

        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Multiplication"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *toggle = new ElaToggleSwitch(m_settingsPage);
        toggle->setIsToggled(initial);
        connect(toggle, &ElaToggleSwitch::toggled, [this](bool checked){
            for (auto &env : m_env) env->allows_breeding = checked;
            m_settings.setValue("multiplicationEnabled", QVariant::fromValue(checked));
        });
        row->addWidget(toggle);
        settingsLayout->addWidget(area);
    }

    // --- Windowed Mode ---
    {
        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Windowed Mode"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *toggle = new ElaToggleSwitch(m_settingsPage);
        toggle->setIsToggled(windowedMode());

        if (!m_windowedModeAction) m_windowedModeAction = new QAction(this);
        m_windowedModeAction->setCheckable(true);
        m_windowedModeAction->setChecked(windowedMode());

        connect(toggle, &ElaToggleSwitch::toggled, [this](bool checked){
            setWindowedMode(checked);
        });
        connect(m_windowedModeAction, &QAction::toggled, toggle, &ElaToggleSwitch::setIsToggled);
        row->addWidget(toggle);
        settingsLayout->addWidget(area);
    }

    // --- Background Color ---
    {
        static const QString key = "windowedModeBackground";
        QColor initial = m_settings.value(key, "#FF0000").toString();
        m_sandboxBackground = initial;
        updateSandboxBackground();

        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Background Color"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *btn = new ElaPushButton("...", m_settingsPage);
        connect(btn, &ElaPushButton::clicked, [this](){
            QColorDialog dialog { this };
            dialog.setCurrentColor(m_sandboxBackground);
            if (dialog.exec() == 1) {
                m_sandboxBackground = dialog.selectedColor();
                m_settings.setValue("windowedModeBackground", colorToString(dialog.selectedColor()));
                updateSandboxBackground();
            }
        });
        row->addWidget(btn);
        settingsLayout->addWidget(area);
    }

    // --- Scale ---
    {
        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Scale"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *btn = new ElaPushButton("...", m_settingsPage);
        connect(btn, &ElaPushButton::clicked, [this](){
            static const QString key = "userScale";
            QDialog dialog { this };
            dialog.setWindowTitle(tr("Custom Scale"));
            dialog.setMinimumWidth(300);
            auto mainLayout = new QVBoxLayout(&dialog);

            auto slider = new QSlider(Qt::Horizontal);
            slider->setRange(100, 10000);
            slider->setValue(m_userScale * 1000);

            auto spin = new QDoubleSpinBox;
            spin->setRange(0.1, 10.0);
            spin->setDecimals(3);
            spin->setSingleStep(0.05);
            spin->setValue(m_userScale);

            connect(slider, &QSlider::valueChanged, [this, spin](int v){
                m_userScale = v/1000.0;
                spin->blockSignals(true);
                spin->setValue(m_userScale);
                spin->blockSignals(false);
            });
            connect(spin, QOverload<double>::of(&QDoubleSpinBox::valueChanged), [this, slider](double v){
                m_userScale = v;
                slider->blockSignals(true);
                slider->setValue(v*1000);
                slider->blockSignals(false);
            });

            mainLayout->addWidget(new QLabel(tr("Adjust Scale:")));
            mainLayout->addWidget(slider);
            mainLayout->addWidget(spin);

            auto btnSave = new QPushButton(tr("Save"));
            connect(btnSave, &QPushButton::clicked, &dialog, &QDialog::accept);
            mainLayout->addWidget(btnSave);

            if (dialog.exec() == QDialog::Accepted) {
                m_settings.setValue(key, m_userScale);
            }
        });
        row->addWidget(btn);
        settingsLayout->addWidget(area);
    }

    // --- Language ---
    {
        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Language"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *btn = new ElaPushButton("...", m_settingsPage);
        connect(btn, &ElaPushButton::clicked, [this](){
            QDialog d(this);
            d.setWindowTitle(tr("Select Language"));
            auto l = new QVBoxLayout(&d);

            auto btnEn = new QRadioButton("English");
            auto btnZh = new QRadioButton("中文(简体)");

            if (m_currentLanguage == "zh_CN") btnZh->setChecked(true);
            else btnEn->setChecked(true);

            l->addWidget(btnEn);
            l->addWidget(btnZh);

            auto bb = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
            connect(bb, &QDialogButtonBox::accepted, &d, &QDialog::accept);
            connect(bb, &QDialogButtonBox::rejected, &d, &QDialog::reject);
            l->addWidget(bb);

            if (d.exec() == QDialog::Accepted) {
                if (btnZh->isChecked()) switchLanguage("zh_CN");
                else switchLanguage("en");
            }
        });
        row->addWidget(btn);
        settingsLayout->addWidget(area);
    }

    // --- Detach Speed ---
    {
        auto *area = new ElaScrollPageArea(m_settingsPage);
        auto *row = new QHBoxLayout(area);
        auto *label = new ElaText(tr("Detach Speed"), m_settingsPage);
        label->setWordWrap(false);
        label->setTextPixelSize(15);
        row->addWidget(label);
        row->addStretch();
        auto *btn = new ElaPushButton("...", m_settingsPage);
        connect(btn, &ElaPushButton::clicked, [this](){
            static const QString key = "detachThreshold";
            QDialog dialog(this);
            dialog.setWindowTitle(tr("Detach Speed"));
            auto l = new QVBoxLayout(&dialog);

            auto slider = new QSlider(Qt::Horizontal);
            slider->setRange(0, 200);
            slider->setValue(m_detachThreshold);

            auto spin = new QSpinBox;
            spin->setRange(0, 200);
            spin->setValue(m_detachThreshold);

            connect(slider, &QSlider::valueChanged, spin, &QSpinBox::setValue);
            connect(spin, QOverload<int>::of(&QSpinBox::valueChanged), slider, &QSlider::setValue);

            l->addWidget(new QLabel(tr("Threshold (px/tick):")));
            l->addWidget(slider);
            l->addWidget(spin);

            auto btnSave = new QPushButton(tr("Save"));
            connect(btnSave, &QPushButton::clicked, &dialog, &QDialog::accept);
            l->addWidget(btnSave);

            if (dialog.exec() == QDialog::Accepted) {
                m_detachThreshold = spin->value();
                m_settings.setValue(key, m_detachThreshold);
            }
        });
        row->addWidget(btn);
        settingsLayout->addWidget(area);
    }

    settingsLayout->addStretch();
    addFooterNode(tr("Settings"), m_settingsPage, m_settingsKey, 0, ElaIconType::GearComplex);

    // ==================== ABOUT (footer, no page) ====================
    addFooterNode(tr("About"), m_aboutKey, 0, ElaIconType::User);
    connect(this, &ElaWindow::navigationNodeClicked, [this](ElaNavigationType::NavigationNodeType nodeType, QString nodeKey){
        if (nodeKey != m_aboutKey) return;
        Q_UNUSED(nodeType);

        QString version = QStringLiteral(NEUROLINGSCE_VERSION);
        QString authorName = QString::fromUtf8("\xe8\xbd\xbb\xe5\xb0\x98\xe5\x91\xa6");

        QDialog *aboutDialog = new QDialog(this);
        aboutDialog->setWindowTitle(tr("About NeurolingsCE"));
        aboutDialog->setFixedWidth(420);
        aboutDialog->setAttribute(Qt::WA_DeleteOnClose);

        // Theme-aware colors
        auto themeMode = eTheme->getThemeMode();
        QString primaryColor = ElaThemeColor(themeMode, PrimaryNormal).name();
        QString textColor = ElaThemeColor(themeMode, BasicText).name();
        QString detailsColor = ElaThemeColor(themeMode, BasicDetailsText).name();
        QString cardBg = ElaThemeColor(themeMode, BasicBase).name();
        QString cardBorder = ElaThemeColor(themeMode, BasicBorder).name();
        QString dialogBg = ElaThemeColor(themeMode, DialogBase).name();
        QString badgeBg = ElaThemeColor(themeMode, BasicHover).name();

        aboutDialog->setStyleSheet(QString("QDialog { background-color: %1; }").arg(dialogBg));

        QVBoxLayout *layout = new QVBoxLayout;
        layout->setSpacing(12);
        layout->setContentsMargins(24, 24, 24, 24);
        aboutDialog->setLayout(layout);

        // App icon
        QLabel *iconLabel = new QLabel;
        QIcon appIcon = qApp->windowIcon();
        if (!appIcon.isNull()) {
            iconLabel->setPixmap(appIcon.pixmap(64, 64));
        }
        iconLabel->setAlignment(Qt::AlignCenter);
        layout->addWidget(iconLabel);

        // Title
        QLabel *titleLabel = new QLabel(
            QString("<h2 style='color: %1; margin: 0;'>NeurolingsCE</h2>").arg(primaryColor));
        titleLabel->setAlignment(Qt::AlignCenter);
        layout->addWidget(titleLabel);

        // Version badge
        QLabel *versionLabel = new QLabel(
            QString("<span style='background-color: %1; color: %2; "
                "padding: 3px 12px; border-radius: 10px; font-size: 12px;'>v%3</span>")
            .arg(badgeBg, primaryColor, version));
        versionLabel->setAlignment(Qt::AlignCenter);
        layout->addWidget(versionLabel);

        // Description
        QLabel *descLabel = new QLabel(
            tr("A cross-platform shimeji desktop pet runner."));
        descLabel->setAlignment(Qt::AlignCenter);
        descLabel->setStyleSheet(QString("color: %1; margin: 4px 0;").arg(detailsColor));
        layout->addWidget(descLabel);

        // Info card
        QString infoHtml = QStringLiteral(
            "<table cellpadding='4' style='color: %1;'>"
            "<tr><td style='color: %2; font-weight: bold;'>%3</td>"
            "<td><a href='https://space.bilibili.com/178381315' "
            "style='color: %2; text-decoration: none;'>%4</a></td></tr>"
            "<tr><td style='color: %2; font-weight: bold;'>%5</td>"
            "<td><a href='https://github.com/pixelomer/Shijima-Qt' "
            "style='color: %2; text-decoration: none;'>Shijima-Qt</a> by pixelomer</td></tr>"
            "<tr><td style='color: %2; font-weight: bold;'>%6</td>"
            "<td><a href='https://github.com/qingchenyouforcc/NeurolingsCE' "
            "style='color: %2; text-decoration: none;'>GitHub</a></td></tr>"
            "<tr><td style='color: %2; font-weight: bold;'>%7</td>"
            "<td>423902950</td></tr>"
            "<tr><td style='color: %2; font-weight: bold;'>%8</td>"
            "<td>125081756</td></tr>"
            "</table>")
            .arg(textColor, primaryColor,
                 tr("Author"), authorName,
                 tr("Based on"),
                 tr("Project"),
                 tr("Feedback QQ"),
                 tr("Chat QQ"));
        QLabel *infoLabel = new QLabel(infoHtml);
        infoLabel->setOpenExternalLinks(true);
        infoLabel->setAlignment(Qt::AlignCenter);
        infoLabel->setStyleSheet(QString("background-color: %1; border: 1px solid %2; "
            "border-radius: 8px; padding: 12px;").arg(cardBg, cardBorder));
        layout->addWidget(infoLabel);

        // Button stylesheet for theme consistency
        QString buttonStyle = QString(
            "QPushButton { background-color: %1; color: %2; border: 1px solid %3; "
            "border-radius: 6px; padding: 6px 16px; font-size: 13px; }"
            "QPushButton:hover { background-color: %4; }"
            "QPushButton:pressed { background-color: %5; }"
        ).arg(cardBg, textColor, cardBorder,
             ElaThemeColor(themeMode, BasicHover).name(),
             ElaThemeColor(themeMode, BasicPress).name());

        // View Licenses button
        QPushButton *licensesButton = new QPushButton(tr("View Licenses"));
        licensesButton->setStyleSheet(buttonStyle);
        connect(licensesButton, &QPushButton::clicked, [this](){
            ShijimaLicensesDialog dialog { this };
            dialog.exec();
        });
        layout->addWidget(licensesButton);

        // Links row
        QHBoxLayout *linksRow = new QHBoxLayout;
        linksRow->addStretch();
        auto *btnWeb = new QPushButton(tr("Shijima Website"));
        btnWeb->setStyleSheet(buttonStyle);
        connect(btnWeb, &QPushButton::clicked, [](){
            QDesktopServices::openUrl(QUrl { "https://getshijima.app" });
        });
        linksRow->addWidget(btnWeb);
        auto *btnBug = new QPushButton(tr("Report Issue"));
        btnBug->setStyleSheet(buttonStyle);
        connect(btnBug, &QPushButton::clicked, [](){
            QDesktopServices::openUrl(QUrl { "https://github.com/qingchenyouforcc/NeurolingsCE/issues" });
        });
        linksRow->addWidget(btnBug);
        linksRow->addStretch();
        layout->addLayout(linksRow);

        // Close button
        QHBoxLayout *buttonRow = new QHBoxLayout;
        buttonRow->addStretch();
        QPushButton *closeButton = new QPushButton(tr("Close"));
        closeButton->setStyleSheet(buttonStyle);
        closeButton->setMinimumWidth(100);
        connect(closeButton, &QPushButton::clicked, aboutDialog, &QDialog::accept);
        buttonRow->addWidget(closeButton);
        buttonRow->addStretch();
        layout->addLayout(buttonRow);

        aboutDialog->exec();
    });
}

void ShijimaManager::refreshListWidget() {
    //FIXME: refresh only changed items
    m_listWidget.clear();
    auto names = m_loadedMascots.keys();
    names.sort(Qt::CaseInsensitive);
    for (auto &name : names) {
        auto item = new QListWidgetItem;
        item->setText(name);
        item->setIcon(m_loadedMascots[name]->preview());
        m_listWidget.addItem(item);
    }
    m_listItemsToRefresh.clear();
}

void ShijimaManager::loadAllMascots() {
    QDirIterator iter { m_mascotsPath, QDir::Dirs | QDir::NoDotAndDotDot,
        QDirIterator::NoIteratorFlags };
    while (iter.hasNext()) {
        auto name = iter.nextFileInfo().fileName();
        if (!name.endsWith(".mascot") || name.length() <= 7) {
            continue;
        }
        reloadMascot(name.sliced(0, name.length() - 7));
    }
    refreshListWidget();
}

void ShijimaManager::reloadMascots(std::set<std::string> const& mascots) {
    for (auto &mascot : mascots) {
        reloadMascot(QString::fromStdString(mascot));
    }
    refreshListWidget();
}

std::set<std::string> ShijimaManager::import(QString const& path) noexcept {
    try {
#if !SHIJIMA_WITH_SHIMEJIFINDER
        return SimpleZipImporter::import(path, m_mascotsPath);
#else
        auto ar = shimejifinder::analyze(path.toStdString());
        ar->extract(m_mascotsPath.toStdString());
        return ar->shimejis();
#endif
    }
    catch (std::exception &ex) {
        std::cerr << "import failed: " << ex.what() << std::endl;
        return {};
    }
}

void ShijimaManager::importWithDialog(QList<QString> const& paths) {
    ForcedProgressDialog *dialog = new ForcedProgressDialog { this };
    dialog->setRange(0, 0);
    QPushButton *cancelButton = new QPushButton;
    cancelButton->setEnabled(false);
    cancelButton->setText(tr("Cancel"));
    dialog->setModal(true);
    dialog->setCancelButton(cancelButton);
    dialog->setLabelText(tr("Importing shimeji..."));
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->show();
    //hide();
    QtConcurrent::run([this, paths](){
        std::set<std::string> changed;
        for (auto &path : paths) {
            auto newChanged = import(path);
            changed.insert(newChanged.begin(), newChanged.end());
        }
        return changed;
    }).then([this, dialog](std::set<std::string> changed){
        dispatchToMainThread([this, changed, dialog](){
            reloadMascots(changed);
            this->show();
            dialog->close();
            QString msg;
            QMessageBox::Icon icon;
            if (changed.size() > 0) {
                msg = tr("Imported %n mascot(s).", "", (int)changed.size());
                icon = QMessageBox::Icon::Information;
            }
            else {
                msg = tr("Could not import any mascots from the specified archive(s).");
                icon = QMessageBox::Icon::Warning;
            }
            QMessageBox msgBox { icon, tr("Import"), msg,
                QMessageBox::StandardButton::Ok, this };
            msgBox.exec();
        });
    });
}

void ShijimaManager::showEvent(QShowEvent *event) {
    PlatformWidget::showEvent(event);
    if (!m_firstShow) {
        return;
    }
    m_firstShow = false;
    if (!m_importOnShowPath.isEmpty()) {
        QString path = m_importOnShowPath;
        m_importOnShowPath = {};
        importWithDialog({ path });
    }
    else {
        if (m_loadedMascots.size() == 1) {
            auto msgBox = new QMessageBox { this };
            msgBox->setText(tr("Welcome to NeurolingsCE! Get started by dragging and dropping a "
                "shimeji archive to the manager window. You can also import archives "
                "by selecting File > Import."));
            msgBox->addButton(QMessageBox::StandardButton::Ok);
            msgBox->setAttribute(Qt::WA_DeleteOnClose);
            msgBox->show();
        }
    }
}

void ShijimaManager::importOnShow(QString const& path) {
    m_importOnShowPath = path;
}

void ShijimaManager::dragEnterEvent(QDragEnterEvent *event) {
    if (event->mimeData()->hasUrls()) {
        event->acceptProposedAction();
    }
}

void ShijimaManager::dropEvent(QDropEvent *event) {
    QList<QString> paths;
    for (auto &url : event->mimeData()->urls()) {
        paths.append(url.toLocalFile());
    }
    importWithDialog(paths);
}

void ShijimaManager::screenAdded(QScreen *screen) {
    if (!m_env.contains(screen)) {
        auto env = std::make_shared<shijima::mascot::environment>();
        m_env[screen] = env;
        m_reverseEnv[env.get()] = screen;
        auto primary = QGuiApplication::primaryScreen();
        if (screen != primary && m_env.contains(primary)) {
            m_env[screen]->allows_breeding = m_env[primary]->allows_breeding;
        }
    }
}

void ShijimaManager::screenRemoved(QScreen *screen) {
    if (m_env.contains(screen) && screen != nullptr) {
        auto primary = QGuiApplication::primaryScreen();
        for (auto &mascot : m_mascots) {
            mascot->setEnv(m_env[primary]);
            mascot->mascot().reset_position();
        }
        m_reverseEnv.remove(m_env[primary].get());
        m_env.remove(screen);
    }
}

ShijimaManager::~ShijimaManager() {
    disconnect(qApp, &QGuiApplication::screenAdded,
        this, &ShijimaManager::screenAdded);
    disconnect(qApp, &QGuiApplication::screenRemoved,
        this, &ShijimaManager::screenRemoved);
}

void ShijimaManager::onTickSync(std::function<void(ShijimaManager *)> callback) {
    auto lock = acquireLock();
    m_hasTickCallbacks = true;
    m_tickCallbacks.push_back(callback);
    m_tickCallbackCompletion.wait(lock);
}

void ShijimaManager::setWindowedMode(bool windowedMode) {
    if (!!this->windowedMode() == !!windowedMode) {
        // no change
        return;
    }
    m_windowedModeAction->setChecked(windowedMode);
    for (auto mascot : m_mascots) {
        mascot->close();
        mascot->setParent(nullptr);
    }
    if (windowedMode) {
        QWidget *parent;
        #if defined(_WIN32)
            parent = nullptr;
        #else
            parent = this;
        #endif
        m_sandboxWidget = new QWidget { parent, Qt::Window };
        m_sandboxWidget->setAttribute(Qt::WA_StyledBackground, true);
        m_sandboxWidget->resize(640, 480);
        m_sandboxWidget->setObjectName("sandboxWindow");
        m_sandboxWidget->show();
        updateSandboxBackground();
    }
    else {
        m_sandboxWidget->close();
        delete m_sandboxWidget;
        m_sandboxWidget = nullptr;
    }
    updateEnvironment();
    std::shared_ptr<shijima::mascot::environment> env;
    if (windowedMode) {
        env = m_env[nullptr];
    }
    else {
        env = m_env[mascotScreen()];
    }
    for (auto &mascot : m_mascots) {
        bool inspectorWasVisible = mascot->inspectorVisible();
        auto newMascot = new ShijimaWidget(*mascot, windowedMode,
            mascotParent());
        newMascot->setEnv(env);
        delete mascot;
        mascot = newMascot;
        m_mascotsById[mascot->mascotId()] = mascot;
        mascot->mascot().reset_position();
        mascot->show();
        if (inspectorWasVisible) {
            mascot->showInspector();
        }
    }
}

void ShijimaManager::updateStatusBar() {
    if (m_statusLabel == nullptr) {
        return;
    }
    int mascotCount = static_cast<int>(m_mascots.size());
    int templateCount = m_loadedMascots.size();
    m_statusLabel->setText(tr("  Mascots: %1  |  Templates: %2")
        .arg(mascotCount).arg(templateCount));
}

ShijimaManager::ShijimaManager(QWidget *parent):
    PlatformWidget(parent, PlatformWidget::ShowOnAllDesktops),
    m_sandboxWidget(nullptr),
    m_settings("pixelomer", "Shijima-Qt"),
    m_windowedModeAction(nullptr),
    m_idCounter(0), m_httpApi(this),
    m_hasTickCallbacks(false),
    m_translator(nullptr),
    m_qtTranslator(nullptr),
    m_currentLanguage("en")
{
    for (auto screen : QGuiApplication::screens()) {
        screenAdded(screen);
    }
    screenAdded(nullptr);

    connect(qApp, &QGuiApplication::screenAdded,
        this, &ShijimaManager::screenAdded);
    connect(qApp, &QGuiApplication::screenRemoved,
        this, &ShijimaManager::screenRemoved);

    QString dataPath = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    QString mascotsPath = QDir::cleanPath(dataPath + QDir::separator() + "mascots");
    QDir mascotsDir(mascotsPath);
    if (!mascotsDir.exists()) {
        mascotsDir.mkpath(mascotsPath);
    }
    if (QFile readme { mascotsDir.absoluteFilePath("README.txt") };
        readme.open(QFile::WriteOnly | QFile::NewOnly | QFile::Text))
    {
        readme.write(""
"Manually importing shimeji by copying its contents into this folder may\n"
"cause problems. You should use the import dialog in Shijima-Qt unless you\n"
"have a good reason not to.\n"
        );
        readme.close();
    }
    m_mascotsPath = mascotsPath;
    std::cout << "Mascots path: " << m_mascotsPath.toStdString() << std::endl;
    
    loadDefaultMascot();
    loadAllMascots();
    setAcceptDrops(true);

    m_mascotTimer = startTimer(40 / SHIJIMAQT_SUBTICK_COUNT);
    if (m_windowObserver.tickFrequency() > 0) {
        m_windowObserverTimer = startTimer(m_windowObserver.tickFrequency());
    }
    // ElaWindow setup
    setUserInfoCardVisible(false);
    setWindowButtonFlag(ElaAppBarType::StayTopButtonHint, false);
    setWindowButtonFlag(ElaAppBarType::MaximizeButtonHint, false);
    setIsDefaultClosed(false);
    connect(this, &ElaWindow::closeButtonClicked, this, [this]() {
        #if defined(_WIN32)
        if (m_mascots.size() == 0) {
            askClose();
        } else {
            setManagerVisible(false);
        }
        #else
        askClose();
        #endif
    });

    // Sync theme with system color scheme
    auto syncTheme = [](Qt::ColorScheme scheme) {
        eTheme->setThemeMode(scheme == Qt::ColorScheme::Dark
            ? ElaThemeType::Dark : ElaThemeType::Light);
    };
    syncTheme(QGuiApplication::styleHints()->colorScheme());
    connect(QGuiApplication::styleHints(), &QStyleHints::colorSchemeChanged,
            this, syncTheme);

    connect(&m_listWidget, &QListWidget::itemDoubleClicked,
        this, &ShijimaManager::itemDoubleClicked);
    m_listWidget.setIconSize({ 64, 64 });
    m_listWidget.installEventFilter(this);
    m_listWidget.setSelectionMode(QListWidget::ExtendedSelection);

    // Apply theme-aware styling to QListWidget
    auto applyListTheme = [this]() {
        auto mode = eTheme->getThemeMode();
        QColor bg = eTheme->getThemeColor(mode, ElaThemeType::WindowBase);
        QColor text = eTheme->getThemeColor(mode, ElaThemeType::BasicText);
        QColor bgAlt = eTheme->getThemeColor(mode, ElaThemeType::BasicBase);
        QColor hover = eTheme->getThemeColor(mode, ElaThemeType::BasicHover);
        QColor selected = eTheme->getThemeColor(mode, ElaThemeType::PrimaryNormal);
        QColor border = eTheme->getThemeColor(mode, ElaThemeType::BasicBorder);
        m_listWidget.setStyleSheet(QString(
            "QListWidget {"
            "  background-color: %1;"
            "  color: %2;"
            "  border: 1px solid %3;"
            "  border-radius: 6px;"
            "  outline: none;"
            "}"
            "QListWidget::item {"
            "  padding: 4px;"
            "  border-radius: 4px;"
            "}"
            "QListWidget::item:hover {"
            "  background-color: %4;"
            "}"
            "QListWidget::item:selected {"
            "  background-color: %5;"
            "  color: white;"
            "}"
            "QScrollBar:vertical {"
            "  background: %6;"
            "  width: 8px;"
            "  border-radius: 4px;"
            "}"
            "QScrollBar::handle:vertical {"
            "  background: %7;"
            "  min-height: 30px;"
            "  border-radius: 4px;"
            "}"
            "QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {"
            "  height: 0px;"
            "}"
        ).arg(bg.name(), text.name(), border.name(),
              hover.name(), selected.name(), bgAlt.name(),
              border.name()));
    };
    applyListTheme();
    connect(eTheme, &ElaTheme::themeModeChanged, this, [applyListTheme]() {
        applyListTheme();
    });


    // Window title and status bar
    setWindowTitle(tr("NeurolingsCE \u2014 Mascot Manager"));
    auto *elaStatusBar = new ElaStatusBar(this);
    setStatusBar(elaStatusBar);
    m_statusLabel = new QLabel(this);
    elaStatusBar->addWidget(m_statusLabel, 1);
    updateStatusBar();

    // Load saved language before building UI
    QString savedLang = m_settings.value("language", "en").toString();
    if (savedLang != "en") {
        m_currentLanguage = "en"; // force switchLanguage to execute
        switchLanguage(savedLang);
    }

    // Load detachment threshold setting
    m_detachThreshold = m_settings.value("detachThreshold",
        QVariant::fromValue(30.0)).toDouble();

    setupNavigation();
    setManagerVisible(true);
    m_constructing = false;

    setupTrayIconFor(this);

    m_httpApi.start("127.0.0.1", 32456);
}

void ShijimaManager::itemDoubleClicked(QListWidgetItem *qItem) {
    spawn(qItem->text().toStdString());
}

void ShijimaManager::closeEvent(QCloseEvent *event) {
    #if !defined(__APPLE__)
    if (!m_allowClose) {
        event->ignore();
        #if defined(_WIN32)
        if (m_mascots.size() == 0) {
            askClose();
        }
        else {
            setManagerVisible(false);
        }
        #else
        askClose();
        #endif
        return;
    }
    // Stop HTTP server and timers early to avoid blocking on destruction
    m_httpApi.stop();
    if (m_mascotTimer != 0) {
        killTimer(m_mascotTimer);
        m_mascotTimer = 0;
    }
    if (m_windowObserverTimer != 0) {
        killTimer(m_windowObserverTimer);
        m_windowObserverTimer = 0;
    }
    event->accept();
    #else
    event->ignore();
    setManagerVisible(false);
    #endif
}

void ShijimaManager::timerEvent(QTimerEvent *event) {
    int timerId = event->timerId();
    if (timerId == m_mascotTimer) {
        tick();
    }
    else if (timerId == m_windowObserverTimer) {
        m_windowObserver.tick();
    }
}

void ShijimaManager::updateEnvironment(QScreen *screen) {
    if (!m_env.contains(screen)) {
        return;
    }
    auto &env = m_env[screen];
    QRect geometry, available;
    QPoint cursor;
    if (screen == nullptr) {
        if (m_sandboxWidget != nullptr) {
            geometry = m_sandboxWidget->geometry();
            cursor = m_sandboxWidget->cursor().pos() - geometry.topLeft();
            geometry.setCoords(0, 0, geometry.width(), geometry.height());
            available = geometry;
        }
        else {
            std::cerr << "warning: sandboxWidget is not initialized" << std::endl;
        }
    }
    else {
        cursor = QCursor::pos();
        geometry = screen->geometry();
        available = screen->availableGeometry();
    }
    int taskbarHeight = geometry.bottom() - available.bottom();
    int statusBarHeight = available.top() - geometry.top();
    if (taskbarHeight < 0) {
        taskbarHeight = 0;
    }
    if (statusBarHeight < 0) {
        statusBarHeight = 0;
    }
    env->screen = { (double)geometry.top() + statusBarHeight,
        (double)geometry.right(),
        (double)geometry.bottom(),
        (double)geometry.left() };
    env->floor = { (double)geometry.bottom() - taskbarHeight,
        (double)geometry.left(), (double)geometry.right() };
    env->work_area = { (double)geometry.top(),
        (double)geometry.right(),
        (double)geometry.bottom() - taskbarHeight,
        (double)geometry.left() };
    env->ceiling = { (double)geometry.top(), (double)geometry.left(),
        (double)geometry.right() };
    if (!windowedMode() && m_currentWindow.available &&
        std::fabs(m_currentWindow.x) > 1 && std::fabs(m_currentWindow.y) > 1)
    {
        env->active_ie = { m_currentWindow.y,
            m_currentWindow.x + m_currentWindow.width,
            m_currentWindow.y + m_currentWindow.height,
            m_currentWindow.x };
        if (m_previousWindow.available &&
            m_previousWindow.uid == m_currentWindow.uid)
        {
            env->active_ie.dy = m_currentWindow.y - m_previousWindow.y;
            if (env->active_ie.dy == 0) {
                env->active_ie.dy = m_currentWindow.height - m_previousWindow.height;
            }
            env->active_ie.dx = m_currentWindow.x - m_previousWindow.x;
            if (env->active_ie.dx == 0) {
                env->active_ie.dx = m_currentWindow.width - m_previousWindow.width;
            }

            // Gradual detachment: dampen dx/dy based on window speed
            if (m_detachThreshold > 0) {
                double speed = std::sqrt(env->active_ie.dx * env->active_ie.dx
                    + env->active_ie.dy * env->active_ie.dy);
                double upperBound = m_detachThreshold * 3.0;
                if (speed >= upperBound) {
                    // Full detachment: window moved too fast
                    env->active_ie = { -50, -50, -50, -50 };
                }
                else if (speed > m_detachThreshold) {
                    // Partial damping: linearly reduce follow ratio
                    double ratio = 1.0 - (speed - m_detachThreshold)
                        / (upperBound - m_detachThreshold);
                    env->active_ie.dx *= ratio;
                    env->active_ie.dy *= ratio;
                }
            }
        }
    }
    else {
        env->active_ie = { -50, -50, -50, -50 };
    }
    int x = cursor.x(), y = cursor.y();
    env->cursor = { (double)x, (double)y, x - env->cursor.x, y - env->cursor.y };
    env->subtick_count = SHIJIMAQT_SUBTICK_COUNT;

    env->set_scale(1.0 / std::sqrt(m_userScale));
}

void ShijimaManager::updateEnvironment() {
    m_currentWindow = m_windowObserver.getActiveWindow();
    if (windowedMode()) {
        updateEnvironment(nullptr);
    }
    else {
        for (auto screen : QGuiApplication::screens()) {
            updateEnvironment(screen);
        }
    }
    m_previousWindow = m_currentWindow;
}

void ShijimaManager::askClose() {
    setManagerVisible(true);
    QMessageBox msgBox { this };
    msgBox.setWindowTitle(tr("Close NeurolingsCE"));
    msgBox.setIcon(QMessageBox::Icon::Question);
    msgBox.setStandardButtons(QMessageBox::StandardButton::Yes |
        QMessageBox::StandardButton::No);
    msgBox.setText(tr("Do you want to close NeurolingsCE?"));
    int ret = msgBox.exec();
    if (ret == QMessageBox::Button::Yes) {
        #if defined(__APPLE__)
        QCoreApplication::quit();
        #else
        m_allowClose = true;
        closeWindow();
        #endif
    }
}

void ShijimaManager::setManagerVisible(bool visible) {
    if (!m_wasVisible && visible) {
        show();
        if (window() != nullptr) {
            window()->activateWindow();
        }
        m_wasVisible = true;
    }
    else if (m_wasVisible && !visible) {
        #if defined(__APPLE__)
        if (m_mascots.size() == 0) {
            askClose();
            return;
        }
        #endif
        hide();
        clearFocus();
        m_wasVisible = false;
    }
}

bool ShijimaManager::windowedMode() {
    return m_sandboxWidget != nullptr;
}

QWidget *ShijimaManager::mascotParent() {
    if (windowedMode()) {
        return m_sandboxWidget;
    }
    else {
        return this;
    }
}

void ShijimaManager::tick() {
    if (m_hasTickCallbacks) {
        auto lock = acquireLock();
        for (auto &callback : m_tickCallbacks) {
            callback(this);
        }
        m_tickCallbacks.clear();
        m_hasTickCallbacks = false;
        m_tickCallbackCompletion.notify_all();
    }

    if (m_sandboxWidget != nullptr && !m_sandboxWidget->isVisible()) {
        setWindowedMode(false);
        #if !defined(__APPLE__)
        if (m_mascots.size() == 0) {
            setManagerVisible(true);
        }
        #endif
    }

    #if !defined(__APPLE__)
    if (isMinimized()) {
        setWindowState(windowState() & ~Qt::WindowMinimized);
        setManagerVisible(!m_wasVisible);
    }
    else if (isMaximized()) {
        setManagerVisible(true);
    }
    #endif

    if (m_mascots.size() == 0) {
        #if !defined(__APPLE__)
        if (!windowedMode() && (isMinimized() || !m_wasVisible)) {
            setWindowState(windowState() & ~Qt::WindowMinimized);
            setManagerVisible(true);
        }
        #endif
        return;
    }

    updateEnvironment();

    for (auto iter = m_mascots.end(); iter != m_mascots.begin(); ) {
        --iter;
        ShijimaWidget *shimeji = *iter;
        if (!shimeji->isVisible()) {
            int mascotId = shimeji->mascotId();
            delete shimeji;
            auto erasePos = iter;
            ++iter;
            m_mascots.erase(erasePos);
            m_mascotsById.erase(mascotId);
            continue;
        }
        // --- Fall-through tracking ---
        // Before tick: if mascot is in fall-through mode, temporarily
        // lower the floor to the absolute screen bottom (past taskbar).
        double savedFloorY = 0.0;
        bool didOverrideFloor = false;
        if (shimeji->isFallThroughMode()) {
            savedFloorY = shimeji->env()->floor.y;
            shimeji->env()->floor.y = shimeji->env()->screen.bottom;
            shimeji->env()->work_area.bottom = shimeji->env()->screen.bottom;
            didOverrideFloor = true;
        }

        double anchorYBefore = shimeji->mascot().state->anchor.y;
        shimeji->tick();

        // After tick: restore floor if overridden, then update fall tracking.
        if (didOverrideFloor) {
            shimeji->env()->floor.y = savedFloorY;
            shimeji->env()->work_area.bottom = savedFloorY;
        }

        // Track falling state for fall-through detection.
        if (!windowedMode()) {
            double anchorYAfter = shimeji->mascot().state->anchor.y;
            bool onLand = shimeji->mascot().state->on_land();
            bool isDragging = shimeji->mascot().state->dragging;

            // Reset fall-through when mascot is dragged (picked up)
            if (isDragging && shimeji->isFallThroughMode()) {
                shimeji->m_fallThroughMode = false;
                shimeji->m_fallTracking = false;
            }

            if (!onLand && !isDragging && anchorYAfter > anchorYBefore) {
                // Mascot is falling
                if (!shimeji->m_fallTracking) {
                    shimeji->m_fallTracking = true;
                    shimeji->m_fallStartY = anchorYBefore;
                }
                double fallDistance = anchorYAfter - shimeji->m_fallStartY;
                if (fallDistance >= 700.0) {
                    shimeji->m_fallThroughMode = true;
                }
            }
            else {
                // Not falling (on land, dragging, or moving upward)
                shimeji->m_fallTracking = false;
            }
        }
        auto &mascot = shimeji->mascot();
        auto &breedRequest = mascot.state->breed_request;
        if (mascot.state->dragging && !windowedMode()) {
            auto oldScreen = m_reverseEnv[mascot.state->env.get()];
            auto newScreen = QGuiApplication::screenAt(QPoint {
                (int)mascot.state->anchor.x, (int)mascot.state->anchor.y });
            if (newScreen != nullptr && oldScreen != newScreen) {
                mascot.state->env = m_env[newScreen];
            }
        }
        if (breedRequest.available) {
            if (breedRequest.name == "") {
                breedRequest.name = shimeji->mascotName().toStdString();
            }
            // only consider the last path component
            breedRequest.name = breedRequest.name.substr(breedRequest.name.rfind('\\')+1);
            breedRequest.name = breedRequest.name.substr(breedRequest.name.rfind('/')+1);
            std::optional<shijima::mascot::factory::product> product;
            try {
                product = m_factory.spawn(breedRequest);
            }
            catch (std::exception &ex) {
                std::cerr << "couldn't fulfill breed request for "
                    << breedRequest.name << std::endl;
                std::cerr << ex.what() << std::endl;
            }
            if (product.has_value()) {
                ShijimaWidget *child = new ShijimaWidget(
                    m_loadedMascots[QString::fromStdString(breedRequest.name)],
                    std::move(product->manager), m_idCounter++,
                    windowedMode(), mascotParent());
                child->setEnv(shimeji->env());
                child->show();
                m_mascots.push_back(child);
                m_mascotsById[child->mascotId()] = child;
            }
            breedRequest.available = false;
        }
    }
    
    for (auto &env : m_env) {
        env->reset_scale();
    }

    if (m_mascots.size() == 0 && !windowedMode()) {
        // All mascots self-destructed, show manager
        setManagerVisible(true);
    }

    updateStatusBar();
}

ShijimaWidget *ShijimaManager::hitTest(QPoint const& screenPos) {
    for (auto mascot : m_mascots) {
        QPoint localPos = { screenPos.x() - mascot->x(),
            screenPos.y() - mascot->y() };
        if (mascot->pointInside(localPos)) {
            return mascot;
        }
    }
    return nullptr;
}

QScreen *ShijimaManager::mascotScreen() {
    QScreen *screen;
    if (windowedMode()) {
        screen = nullptr;
    }
    else {
        screen = this->screen();
        if (screen == nullptr) {
            screen = qApp->primaryScreen();
        }
    }
    return screen;
}

ShijimaWidget *ShijimaManager::spawn(std::string const& name) {
    QScreen *screen = mascotScreen();
    updateEnvironment(screen);
    auto &env = m_env[screen];
    auto product = m_factory.spawn(name, {});
    product.manager->state->env = env;
    product.manager->reset_position();
    ShijimaWidget *shimeji = new ShijimaWidget(
        m_loadedMascots[QString::fromStdString(name)],
        std::move(product.manager), m_idCounter++,
        windowedMode(), mascotParent());
    shimeji->show();
    m_mascots.push_back(shimeji);
    m_mascotsById[shimeji->mascotId()] = shimeji;
    env->reset_scale();
    return shimeji;
}

bool ShijimaManager::eventFilter(QObject *obj, QEvent *event) {
    if (event->type() == QEvent::KeyPress) {
        QKeyEvent *keyEvent = static_cast<QKeyEvent *>(event);
        auto key = keyEvent->key();
        if (key == Qt::Key::Key_Return || key == Qt::Key::Key_Enter) {
            for (auto item : m_listWidget.selectedItems()) {
                itemDoubleClicked(item);
            }
            return true;
        }
    }
    return ElaWindow::eventFilter(obj, event);
}

void ShijimaManager::spawnClicked() {
    auto &allTemplates = m_factory.get_all_templates();
    int target = QRandomGenerator::global()->bounded((int)allTemplates.size());
    int i = 0;
    for (auto &pair : allTemplates) {
        if (i++ != target) continue;
        std::cout << "Spawning: " << pair.first << std::endl;
        spawn(pair.first);
        break;
    }
}

void ShijimaManager::switchLanguage(const QString &langCode) {
    if (langCode == m_currentLanguage) {
        return;
    }
    // Remove existing translators
    if (m_translator != nullptr) {
        qApp->removeTranslator(m_translator);
        delete m_translator;
        m_translator = nullptr;
    }
    if (m_qtTranslator != nullptr) {
        qApp->removeTranslator(m_qtTranslator);
        delete m_qtTranslator;
        m_qtTranslator = nullptr;
    }
    m_currentLanguage = langCode;
    m_settings.setValue("language", langCode);
    // Install new translators (skip for English — that's the source language)
    if (langCode != "en") {
        m_translator = new QTranslator(this);
        if (m_translator->load("shijima-qt_" + langCode, ":/i18n")) {
            qApp->installTranslator(m_translator);
        }
        m_qtTranslator = new QTranslator(this);
        if (m_qtTranslator->load("qt_" + langCode, ":/i18n")) {
            qApp->installTranslator(m_qtTranslator);
        }
    }

    if (!m_constructing) {
        QMessageBox::information(this,
            tr("Language Changed"),
            tr("The application will restart to apply the new language."));

        // Stop single-instance HTTP API first to avoid restart race.
        m_httpApi.stop();

        const QString program = QCoreApplication::applicationFilePath();
        const QStringList args = QCoreApplication::arguments().mid(1);
        QProcess::startDetached(program, args);
        m_allowClose = true;
        closeWindow();
    }
}

void ShijimaManager::retranslateUi() {
    setWindowTitle(tr("NeurolingsCE \u2014 Mascot Manager"));
    updateStatusBar();
    rebuildTrayMenuFor(this);
    // Note: ElaWindow navigation node labels (Home, Settings, About) are set at
    // creation time via addPageNode/addFooterNode and cannot be updated in-place.
    // A full navigation rebuild would require destroying and recreating all pages
    // and their signal connections. For now, navigation labels update on next restart.
}

void ShijimaManager::changeEvent(QEvent *event) {
    if (event->type() == QEvent::LanguageChange && !m_constructing) {
        retranslateUi();
    }
    PlatformWidget::changeEvent(event);
}


#include "ShijimaManager.moc"