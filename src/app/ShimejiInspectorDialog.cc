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

#include "shijima-qt/ShimejiInspectorDialog.hpp"
#include "shijima-qt/ShijimaWidget.hpp"
#include <QFormLayout>
#include <string>
#include <QLabel>
#include <QPoint>
#include "ElaTheme.h"

static std::string doubleToString(double val) {
    auto str = std::to_string(val);
    auto dot = str.rfind('.');
    if (dot != std::string::npos) {
        str = str.substr(0, dot + 3);
    }
    return str;
}

static std::string vecToString(shijima::math::vec2 const& vec) {
    return "x: " + doubleToString(vec.x) +
        ", y: " + doubleToString(vec.y);
}

static std::string vecToString(QPoint const& vec) {
    return "x: " + doubleToString(vec.x()) +
        ", y: " + doubleToString(vec.y());
}

static std::string vecToString(shijima::mascot::environment::dvec2 const& vec) {
    return "x: " + doubleToString(vec.x) +
        ", y: " + doubleToString(vec.y) +
        ", dx: " + doubleToString(vec.dx) +
        ", dy: " + doubleToString(vec.dy);
}

static std::string areaToString(shijima::mascot::environment::area const& area) {
    return "x: " + doubleToString(area.left) +
        ", y: " + doubleToString(area.top) +
        ", width: " + doubleToString(area.width()) +
        ", height: " + doubleToString(area.height());
}

static std::string areaToString(shijima::mascot::environment::darea const& area) {
    return "x: " + doubleToString(area.left) +
        ", y: " + doubleToString(area.top) +
        ", width: " + doubleToString(area.width()) +
        ", height: " + doubleToString(area.height()) + 
        ", dx: " + doubleToString(area.dx) + 
        ", dy: " + doubleToString(area.dy);
}

ShimejiInspectorDialog::ShimejiInspectorDialog(ShijimaWidget *parent):
    QDialog(parent), m_formLayout(new QFormLayout)
{
    setWindowFlags((windowFlags() | Qt::CustomizeWindowHint |
        Qt::WindowCloseButtonHint) &
        ~(Qt::WindowMinimizeButtonHint | Qt::WindowMaximizeButtonHint));
    setWindowTitle(tr("Inspector — %1").arg(parent->mascotName()));
    setStyleSheet(QString("QDialog { background-color: %1; }")
        .arg(ElaThemeColor(eTheme->getThemeMode(), DialogBase).name()));
    setLayout(m_formLayout);
    m_formLayout->setFormAlignment(Qt::AlignLeft);
    m_formLayout->setLabelAlignment(Qt::AlignRight);
    m_formLayout->setHorizontalSpacing(16);
    m_formLayout->setVerticalSpacing(6);
    m_formLayout->setContentsMargins(16, 16, 16, 16);

    addRow(tr("Window"), [this](shijima::mascot::manager &mascot){
        return vecToString(shijimaParent()->pos());
    });
    addRow(tr("Anchor"), [](shijima::mascot::manager &mascot){
        return vecToString(mascot.state->anchor);
    });
    addRow(tr("Cursor"), [](shijima::mascot::manager &mascot){
        return vecToString(mascot.state->get_cursor());
    });
    addRow(tr("Behavior"), [](shijima::mascot::manager &mascot){
        return mascot.active_behavior()->name;
    });
    addRow(tr("Image"), [](shijima::mascot::manager &mascot){
        return mascot.state->active_frame.get_name(mascot.state->looking_right);
    });
    addRow(tr("Screen"), [](shijima::mascot::manager &mascot){
        return areaToString(mascot.state->env->screen);
    });
    addRow(tr("Work Area"), [](shijima::mascot::manager &mascot){
        return areaToString(mascot.state->env->work_area);
    });
    addRow(tr("Active IE"), [](shijima::mascot::manager &mascot){
        if (mascot.state->env->active_ie.visible()) {
            return areaToString(mascot.state->env->active_ie);
        }
        else {
            return std::string { tr("not visible").toStdString() };
        }
    });
}

void ShimejiInspectorDialog::addRow(QString const& label,
    std::function<std::string(shijima::mascot::manager &)> tick)
{
    auto themeMode = eTheme->getThemeMode();
    QString primaryColor = ElaThemeColor(themeMode, PrimaryNormal).name();
    QString textColor = ElaThemeColor(themeMode, BasicText).name();
    QString baseBg = ElaThemeColor(themeMode, BasicBase).name();

    auto labelWidget = new QLabel { label };
    labelWidget->setStyleSheet(QString("font-weight: bold; color: %1;").arg(primaryColor));
    auto dataWidget = new QLabel {};
    dataWidget->setStyleSheet(
        QString("font-family: 'Consolas', 'Cascadia Mono', 'Source Code Pro', monospace;"
        "color: %1; padding: 2px 4px;"
        "background-color: %2; border-radius: 3px;").arg(textColor, baseBg));
    m_tickCallbacks.push_back([this, dataWidget, tick](){
        auto newText = tick(shijimaParent()->mascot());
        dataWidget->setText(QString::fromStdString(newText));
    });
    m_formLayout->addRow(labelWidget, dataWidget);
}

ShijimaWidget *ShimejiInspectorDialog::shijimaParent() {
    return static_cast<ShijimaWidget *>(parent());
}

void ShimejiInspectorDialog::showEvent(QShowEvent *event) {
    //setMinimumSize(size());
    //setMaximumSize(size());
}

void ShimejiInspectorDialog::tick() {
    for (auto &callback : m_tickCallbacks) {
        callback();
    }
}

#include "ShimejiInspectorDialog.moc"
