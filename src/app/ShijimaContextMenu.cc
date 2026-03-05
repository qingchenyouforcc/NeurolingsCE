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

#include "shijima-qt/ShijimaContextMenu.hpp"
#include "shijima-qt/ShijimaWidget.hpp"
#include "shijima-qt/ShijimaManager.hpp"

ShijimaContextMenu::ShijimaContextMenu(ShijimaWidget *parent)
    : QMenu("Context menu", parent)
{
    QAction *action;

    // --- Behaviors submenu ---
    {
        std::vector<std::string> behaviors;
        auto &list = parent->m_mascot->initial_behavior_list();
        auto flat = list.flatten_unconditional();
        for (auto &behavior : flat) {
            if (!behavior->hidden) {
                behaviors.push_back(behavior->name);
            }
        }
        auto behaviorsMenu = addMenu(QString::fromUtf8("\xf0\x9f\x8e\xad ") + tr("Behaviors"));
        for (std::string &behavior : behaviors) {
            action = behaviorsMenu->addAction(QString::fromStdString(behavior));
            connect(action, &QAction::triggered, [this, behavior](){
                shijimaParent()->m_mascot->next_behavior(behavior);
            });
        }
    }

    addSeparator();

    // --- Control group ---
    // Pause checkbox
    action = addAction(QString::fromUtf8("\xe2\x8f\xb8 ") + tr("Pause"));
    action->setCheckable(true);
    action->setChecked(parent->m_paused);
    connect(action, &QAction::triggered, [this](bool checked){
        shijimaParent()->m_paused = checked;
    });

    // Call another
    action = addAction(QString::fromUtf8("\xe2\x9c\xa8 ") + tr("Call another"));
    connect(action, &QAction::triggered, [this](){
        ShijimaManager::defaultManager()->spawn(this->shijimaParent()->mascotName()
            .toStdString());
    });

    addSeparator();

    // --- Management group ---
    // Show manager
    action = addAction(QString::fromUtf8("\xf0\x9f\x93\x8b ") + tr("Show manager"));
    connect(action, &QAction::triggered, [](){
        ShijimaManager::defaultManager()->setManagerVisible(true);
    });

    // Inspect
    action = addAction(QString::fromUtf8("\xf0\x9f\x94\x8d ") + tr("Inspect"));
    connect(action, &QAction::triggered, [this](){
        shijimaParent()->showInspector();
    });

    addSeparator();

    // --- Dismiss group ---
    // Dismiss all but one
    action = addAction(tr("Dismiss all but one"));
    connect(action, &QAction::triggered, [this](){
        ShijimaManager::defaultManager()->killAllButOne(this->shijimaParent());
    });

    // Dismiss all
    action = addAction(tr("Dismiss all"));
    connect(action, &QAction::triggered, [](){
        ShijimaManager::defaultManager()->killAll();
    });

    // Dismiss
    action = addAction(QString::fromUtf8("\xc3\x97 ") + tr("Dismiss"));
    connect(action, &QAction::triggered, parent, &ShijimaWidget::closeAction);
}

void ShijimaContextMenu::closeEvent(QCloseEvent *event) {
    shijimaParent()->contextMenuClosed(event);
    QMenu::closeEvent(event);
}

/*
ShijimaContextMenu::~ShijimaContextMenu() {
    auto allActions = actions();
    for (QAction *action : allActions) {
        removeAction(action);
        delete action;
    }
}
*/

#include "ShijimaContextMenu.moc"
