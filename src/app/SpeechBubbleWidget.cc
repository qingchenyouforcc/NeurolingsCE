// 
// NeurolingsCE - Cross-platform shimeji simulation app for desktop
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

#include "shijima-qt/SpeechBubbleWidget.hpp"
#include <QPainter>
#include <QPainterPath>
#include <QFontMetrics>
#include <QScreen>
#include <QGuiApplication>
#include <QFile>
#include <QTextStream>
#include <QDir>
#include <QStandardPaths>
#include <QRandomGenerator>

QMap<QString, QStringList> SpeechBubbleWidget::s_bubbleTextsCache;

SpeechBubbleWidget::SpeechBubbleWidget(QWidget *parent)
    : QWidget(parent, Qt::ToolTip | Qt::FramelessWindowHint
        | Qt::WindowDoesNotAcceptFocus | Qt::NoDropShadowWindowHint)
{
    setAttribute(Qt::WA_TranslucentBackground);
    setAttribute(Qt::WA_ShowWithoutActivating);
    setAttribute(Qt::WA_DeleteOnClose, false);

    m_hideTimer.setSingleShot(true);
    connect(&m_hideTimer, &QTimer::timeout, this, &SpeechBubbleWidget::hideBubble);
}

void SpeechBubbleWidget::showBubble(const QString &text, const QPoint &anchorScreenPos) {
    m_text = text;
    m_active = true;
    m_anchorScreenPos = anchorScreenPos;

    // Calculate bubble size based on text
    QFont font;
    font.setPixelSize(13);
    QFontMetrics fm(font);

    // Wrap text to a maximum width
    int maxTextWidth = 200;
    QRect textRect = fm.boundingRect(QRect(0, 0, maxTextWidth, 0),
        Qt::TextWordWrap | Qt::AlignLeft, m_text);

    int bubbleWidth = textRect.width() + m_padding * 2 + 4;
    int bubbleHeight = textRect.height() + m_padding * 2 + m_tailHeight + 4;

    // Minimum size
    if (bubbleWidth < 60) bubbleWidth = 60;
    if (bubbleHeight < 40 + m_tailHeight) bubbleHeight = 40 + m_tailHeight;

    setFixedSize(bubbleWidth, bubbleHeight);
    updatePosition(anchorScreenPos);

    show();
    raise();

    // Auto-hide after 3 seconds
    m_hideTimer.start(3000);
}

void SpeechBubbleWidget::hideBubble() {
    m_active = false;
    m_hideTimer.stop();
    hide();
}

void SpeechBubbleWidget::updatePosition(const QPoint &anchorScreenPos) {
    m_anchorScreenPos = anchorScreenPos;
    if (!m_active) return;

    int bubbleWidth = width();
    int bubbleHeight = height();

    // Position bubble above the mascot's anchor point
    int x = anchorScreenPos.x() - bubbleWidth / 2;
    int y = anchorScreenPos.y() - bubbleHeight - 8;

    // Ensure bubble stays on screen
    QScreen *screen = QGuiApplication::screenAt(anchorScreenPos);
    if (screen) {
        QRect screenGeom = screen->availableGeometry();
        if (x < screenGeom.left()) x = screenGeom.left();
        if (x + bubbleWidth > screenGeom.right())
            x = screenGeom.right() - bubbleWidth;
        if (y < screenGeom.top()) {
            // Show below the mascot instead
            y = anchorScreenPos.y() + 16;
        }
    }

    move(x, y);
}

void SpeechBubbleWidget::paintEvent(QPaintEvent *) {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);

    QFont font;
    font.setPixelSize(13);
    painter.setFont(font);

    // Bubble body rect (above the tail)
    QRectF bodyRect(1, 1, width() - 2, height() - m_tailHeight - 2);

    // Draw bubble body with rounded corners
    QPainterPath bubblePath;
    bubblePath.addRoundedRect(bodyRect, m_cornerRadius, m_cornerRadius);

    // Draw tail (small triangle at bottom center)
    QPointF tailLeft(width() / 2.0 - 8, bodyRect.bottom());
    QPointF tailRight(width() / 2.0 + 8, bodyRect.bottom());
    QPointF tailTip(width() / 2.0, height() - 1.0);

    QPainterPath tailPath;
    tailPath.moveTo(tailLeft);
    tailPath.lineTo(tailTip);
    tailPath.lineTo(tailRight);
    tailPath.closeSubpath();

    QPainterPath fullPath = bubblePath.united(tailPath);

    // Fill and stroke
    painter.setPen(QPen(QColor(120, 120, 120), 1.5));
    painter.setBrush(QColor(255, 255, 255, 240));
    painter.drawPath(fullPath);

    // Draw text
    QRectF textRect = bodyRect.adjusted(m_padding, m_padding,
        -m_padding, -m_padding);
    painter.setPen(QColor(40, 40, 40));
    painter.drawText(textRect, Qt::TextWordWrap | Qt::AlignCenter, m_text);
}

static QStringList loadTextsFromFile(const QString &filePath) {
    QStringList texts;
    QFile file(filePath);
    if (file.exists() && file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        QTextStream stream(&file);
        stream.setEncoding(QStringConverter::Utf8);
        while (!stream.atEnd()) {
            QString line = stream.readLine().trimmed();
            if (!line.isEmpty()) {
                texts.append(line);
            }
        }
        file.close();
    }
    return texts;
}

QStringList SpeechBubbleWidget::loadBubbleTexts(const QString &mascotPath) {
    QStringList texts;

    // Priority 1: Per-mascot bubble_context.txt in mascot folder
    if (!mascotPath.isEmpty() && mascotPath != "@") {
        QString mascotBubbleFile = QDir::cleanPath(
            mascotPath + QDir::separator() + "bubble_context.txt");
        texts = loadTextsFromFile(mascotBubbleFile);
        if (!texts.isEmpty()) return texts;
    }

    // Priority 2: User's global bubbles.txt in app data directory
    QString dataPath = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    QString userFile = QDir::cleanPath(dataPath + QDir::separator() + "bubbles.txt");
    texts = loadTextsFromFile(userFile);
    if (!texts.isEmpty()) return texts;

    // Priority 3: Embedded resource
    QFile resFile(":/bubbles.txt");
    if (resFile.open(QIODevice::ReadOnly | QIODevice::Text)) {
        QTextStream stream(&resFile);
        stream.setEncoding(QStringConverter::Utf8);
        while (!stream.atEnd()) {
            QString line = stream.readLine().trimmed();
            if (!line.isEmpty()) {
                texts.append(line);
            }
        }
        resFile.close();
    }

    // Priority 4: Hardcoded fallback
    if (texts.isEmpty()) {
        texts.append("Hello!");
        texts.append("Hi there~");
        texts.append("(^_^)");
    }

    return texts;
}

QString SpeechBubbleWidget::randomBubbleText(const QString &mascotPath) {
    // Use mascotPath as cache key (empty string for global)
    QString cacheKey = mascotPath.isEmpty() ? QStringLiteral("__global__") : mascotPath;
    if (!s_bubbleTextsCache.contains(cacheKey)) {
        s_bubbleTextsCache[cacheKey] = loadBubbleTexts(mascotPath);
    }
    const QStringList &texts = s_bubbleTextsCache[cacheKey];
    if (texts.isEmpty()) {
        return "Hello!";
    }
    int index = QRandomGenerator::global()->bounded(texts.size());
    return texts.at(index);
}

#include "SpeechBubbleWidget.moc"
