#include <QtWidgets>
#include <array>

namespace {
const QColor face("#f0f0f0");
const QColor gridColor("#f0f0f0");

const QColor paper("#ffffff");
constexpr int compactRowHeight = 19;
constexpr int pictureRole = Qt::UserRole;
constexpr int progressRole = Qt::UserRole + 1;
constexpr int childRole = Qt::UserRole + 2;
constexpr int gridRole = Qt::UserRole + 3;

QPixmap glyph(const QString &name, int size, bool enabled = true)
{
    const QString cacheKey = QStringLiteral("newsqueue/%1/%2/%3").arg(name).arg(size).arg(enabled);
    QPixmap cached;
    if (QPixmapCache::find(cacheKey, &cached))
        return cached;
    QImage image(":/icons/" + name + ".png");
    image = image.scaled(size, size, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    if (!enabled) {
        for (int y = 0; y < image.height(); ++y) {
            for (int x = 0; x < image.width(); ++x) {
                const QColor pixel = image.pixelColor(x, y);
                const int gray = qGray(pixel.rgb());
                image.setPixelColor(x, y, QColor(gray, gray, gray, pixel.alpha()));
            }
        }
    }
    cached = QPixmap::fromImage(image);
    QPixmapCache::insert(cacheKey, cached);
    return cached;
}

QIcon icon(const QString &name, int size)
{
    QIcon result;
    result.addPixmap(glyph(name, size), QIcon::Normal);
    result.addPixmap(glyph(name, size, false), QIcon::Disabled);
    return result;
}

class Header final : public QHeaderView
{
public:
    explicit Header(QWidget *parent) : QHeaderView(Qt::Horizontal, parent)
    {
        setFixedHeight(25);
        setSectionResizeMode(Interactive);
        setMinimumSectionSize(24);
        setSectionsMovable(true);
        setDefaultAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    }

protected:
    void paintSection(QPainter *painter, const QRect &rect, int logical) const override
    {
        painter->fillRect(rect, paper);
        painter->setPen(QColor("#e5e5e5"));
        painter->drawLine(rect.topRight() - QPoint(1, 0), rect.bottomRight() - QPoint(1, 0));
        painter->setPen(Qt::black);
        const auto data = model()->headerData(logical, Qt::Horizontal, Qt::TextAlignmentRole);
        const Qt::Alignment alignment = data.isValid()
            ? Qt::Alignment(data.toInt()) : defaultAlignment();
        painter->drawText(rect.adjusted(logical == 0 ? 7 : 5, -1, -7, -1), alignment | Qt::TextSingleLine,
                          model()->headerData(logical, Qt::Horizontal).toString());
    }
};

class CellPainter final : public QStyledItemDelegate
{
public:
    explicit CellPainter(QObject *parent) : QStyledItemDelegate(parent) {}

    void paint(QPainter *painter, const QStyleOptionViewItem &option,
               const QModelIndex &index) const override
    {
        QStyleOptionViewItem cell(option);
        initStyleOption(&cell, index);
        const QRect rect = cell.rect;
        painter->save();
        painter->setClipRect(rect);
        const bool selected = cell.state.testFlag(QStyle::State_Selected);
        painter->fillRect(rect, selected ? QColor("#cce8ff") : paper);
        if (index.data(gridRole).toBool()) {
            painter->setPen(gridColor);
            painter->drawLine(rect.topRight(), rect.bottomRight());
        }
        QRect text = rect.adjusted(6, 0, -6, -1);
        const QString image = index.data(pictureRole).toString();
        if (!image.isEmpty()) {
            const int indent = index.data(childRole).toBool() ? 16 : 0;
            painter->drawPixmap(rect.left() + 5 + indent, rect.top() + 1, glyph(image, 16));
            text.setLeft(rect.left() + 24 + indent);
        }
        if (cell.features.testFlag(QStyleOptionViewItem::HasCheckIndicator)) {
            QStyleOptionButton indicator;
            indicator.rect = QRect(rect.left() + 4, rect.top() + 2, 14, 15);
            indicator.state = QStyle::State_Enabled |
                (cell.checkState == Qt::Checked ? QStyle::State_On : QStyle::State_Off);
            QApplication::style()->drawPrimitive(QStyle::PE_IndicatorCheckBox, &indicator, painter);
            text.setLeft(rect.left() + 22);
        }
        painter->setPen(Qt::black);
        const int progress = index.data(progressRole).toInt();
        if (progress) {
            const QRect track = rect.adjusted(5, 2, -5, -3);
            painter->setPen(QColor("#bcbcbc"));
            painter->setBrush(QColor("#e6e6e6"));
            painter->drawRect(track.adjusted(0, 0, -1, 0));
            QRect filled = track.adjusted(1, 1, -1, 0);
            filled.setWidth(track.width() * qAbs(progress) / 100 - 1);
            painter->fillRect(filled, progress < 0 ? QColor("#9d5d00") : QColor("#0f7b0f"));
            painter->setPen(Qt::black);
            painter->drawText(track, Qt::AlignCenter, cell.text);
            painter->setPen(Qt::white);
            painter->setClipRect(filled, Qt::IntersectClip);
            painter->drawText(track, Qt::AlignCenter, cell.text);
        } else {
            const QString label = cell.fontMetrics.elidedText(cell.text, Qt::ElideRight, text.width());
            painter->drawText(text, cell.displayAlignment | Qt::TextSingleLine, label);
        }
        painter->restore();
    }
};

struct Column {
    const char *caption;
    int width;
    bool right = false;
};

class Grid final : public QTableWidget
{
public:
    Grid(std::initializer_list<Column> columns, QWidget *parent) : QTableWidget(parent)
    {
        setColumnCount(static_cast<int>(columns.size()));
        setHorizontalHeader(new Header(this));
        int i = 0;
        for (const auto &column : columns) {
            auto *heading = new QTableWidgetItem(QString::fromUtf8(column.caption));
            heading->setTextAlignment(Qt::AlignVCenter | (column.right ? Qt::AlignRight : Qt::AlignLeft));
            setHorizontalHeaderItem(i, heading);
            setColumnWidth(i, column.width + (i == 0 ? 2 : 0));
            ++i;
        }
        verticalHeader()->hide();
        verticalHeader()->setMinimumSectionSize(compactRowHeight);
        verticalHeader()->setDefaultSectionSize(compactRowHeight);
        verticalHeader()->setSectionResizeMode(QHeaderView::Fixed);
        setItemDelegate(new CellPainter(this));
        setEditTriggers(QAbstractItemView::NoEditTriggers);
        setSelectionBehavior(QAbstractItemView::SelectRows);
        setSelectionMode(QAbstractItemView::SingleSelection);
        setHorizontalScrollMode(QAbstractItemView::ScrollPerPixel);
        setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
        setShowGrid(false);
        setStyleSheet("QTableWidget { border: 1px solid #828790; background: #ffffff; }"
                      "QHeaderView { background: #ffffff; }"
                      "QScrollBar:horizontal { height: 17px; background: #eeeeee; margin: 0 18px; }"
                      "QScrollBar::handle:horizontal { background: #929292; margin: 7px 0 8px; min-width: 20px; }"
                      "QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal { width: 0px; }"
                      "QScrollBar::add-page:horizontal, QScrollBar::sub-page:horizontal { background: #eeeeee; }");
    }

    void setGrid(bool grid)
    {
        showLines = grid;
        for (int row = 0; row < rowCount(); ++row) {
            for (int col = 0; col < columnCount(); ++col)
                item(row, col)->setData(gridRole, grid);
        }
        viewport()->update();
    }

    int addRow(const QStringList &values, const QString &image = {}, bool child = false,
               int progress = 0)
    {
        const int row = rowCount();
        insertRow(row);
        for (int col = 0; col < columnCount(); ++col) {
            auto *cell = new QTableWidgetItem(values.value(col));
            cell->setTextAlignment(horizontalHeaderItem(col)->textAlignment());
            cell->setData(gridRole, showLines);
            if (col == 0) {
                cell->setData(pictureRole, image);
                cell->setData(childRole, child);
            }
            if (col == 6 && progress)
                cell->setData(progressRole, progress);
            setItem(row, col, cell);
        }
        return row;
    }

protected:
    void paintEvent(QPaintEvent *event) override
    {
        QTableWidget::paintEvent(event);
        if (!showLines)
            return;
        QPainter painter(viewport());
        painter.setPen(gridColor);
        const int bottom = rowCount() ? rowViewportPosition(rowCount() - 1) + compactRowHeight : 0;
        const int firstLine = (bottom % compactRowHeight + compactRowHeight - 1) % compactRowHeight;
        for (int y = firstLine; y < viewport()->height(); y += compactRowHeight)
            painter.drawLine(0, y, viewport()->width(), y);
        for (int c = 0; c < columnCount(); ++c) {
            const int x = columnViewportPosition(c) + columnWidth(c) - 1;
            painter.drawLine(x, bottom, x, viewport()->height());
        }
    }

private:
    bool showLines = true;
};

class TabBar final : public QTabBar
{
public:
    explicit TabBar(QWidget *parent) : QTabBar(parent)
    {
        const QStringList labels = {"Download queue", "PAR2", "Log", "RSS"};
        const QStringList icons = {"queue", "shield", "page", "rss"};
        for (int n = 0; n < labels.size(); ++n)
            addTab(icon(icons[n], 16), labels[n]);
        setExpanding(false);
        setFocusPolicy(Qt::NoFocus);
    }

protected:
    QSize tabSizeHint(int index) const override
    {
        constexpr int widths[] = {129, 56, 55, 51};
        return QSize(widths[index], 22);
    }

    void paintEvent(QPaintEvent *) override
    {
        QPainter painter(this);
        painter.fillRect(rect(), face);
        painter.setPen(QColor("#d9d9d9"));
        painter.drawLine(0, 21, width(), 21);
        for (int n = 0; n < count(); ++n) {
            const QRect tab = tabRect(n);
            const bool selected = n == currentIndex();
            const QRect box = tab.adjusted(2, selected ? 0 : 2, 0, 0);
            painter.fillRect(box, selected ? QColor("#f8f8f8") : QColor("#f5f5f5"));
            painter.setPen(QColor("#e0e0e0"));
            painter.drawLine(box.topLeft(), box.topRight());
            painter.drawLine(box.topLeft(), box.bottomLeft());
            painter.drawLine(box.topRight(), box.bottomRight());
            tabIcon(n).paint(&painter, QRect(tab.left() + (n == 1 ? 5 : 8), 2, 16, 16));
            painter.setPen(Qt::black);
            painter.drawText(tab.adjusted(n == 1 ? 26 : 30, -1, -2, -1), Qt::AlignVCenter, tabText(n));
        }
    }
};

class ToolButton final : public QToolButton
{
public:
    ToolButton(const QString &label, const QString &name, bool large, QWidget *parent)
        : QToolButton(parent), image(name), large(large)
    {
        setText(label);
        setAccessibleName(label);
        setToolTip(label);
        setAutoRaise(true);
        setFocusPolicy(Qt::NoFocus);
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        QPainter painter(this);
        if (isEnabled() && (isChecked() || isDown() || underMouse())) {
            painter.setPen(QColor("#78aee3"));
            painter.setBrush(QColor("#cde7ff"));
            painter.drawRoundedRect(rect().adjusted(0, 0, -1, -1), 3, 3);
        }
        const int size = large ? 30 : 16;
        painter.drawPixmap((width() - size) / 2, large ? 0 : (height() - size) / 2,
                           glyph(image, size, isEnabled()));
        if (large) {
            painter.setPen(isEnabled() ? Qt::black : QColor("#a0a0a0"));
            painter.drawText(rect().adjusted(0, 30, 0, 0), Qt::AlignHCenter | Qt::AlignVCenter, text());
        }
    }

private:
    QString image;
    bool large;
};

void line(QWidget *parent, QRect area)
{
    auto *divider = new QFrame(parent);
    divider->setGeometry(area);
    divider->setStyleSheet("background: #a0a0a0;");
}

QLabel *caption(QWidget *parent, const QString &text, QRect area)
{
    auto *label = new QLabel(text, parent);
    label->setGeometry(area);
    return label;
}

QPushButton *push(QWidget *parent, const QString &text, QRect area)
{
    auto *button = new QPushButton(text, parent);
    button->setGeometry(area);
    return button;
}

class MenuStyle final : public QProxyStyle
{
public:
    explicit MenuStyle(QMenuBar *menu) : QProxyStyle(QApplication::style()->name())
    {
        setParent(menu);
    }

    int styleHint(StyleHint hint, const QStyleOption *option, const QWidget *widget,
                  QStyleHintReturn *result) const override
    {
        if (hint == SH_UnderlineShortcut && widget && qobject_cast<const QMenuBar *>(widget))
            return QApplication::keyboardModifiers().testFlag(Qt::AltModifier);
        return QProxyStyle::styleHint(hint, option, widget, result);
    }
};

class Setup final : public QDialog
{
public:
    explicit Setup(QWidget *parent) : QDialog(parent, Qt::Dialog | Qt::WindowTitleHint | Qt::WindowCloseButtonHint)
    {
        setWindowTitle("Setup");
        setStyleSheet("QPushButton { margin: 1px; background: #fdfdfd; border: 1px solid #cfcfcf; border-radius: 4px; }"
                      "QPushButton:default { border-color: #0067c0; }"
                      "QPushButton:hover { background: #f0f7ff; border-color: #0067c0; }"
                      "QPushButton:pressed { background: #e0efff; }"
                      "QPushButton:disabled { background: #f8f8f8; color: #808080; border-color: #e5e5e5; }"
                      "QCheckBox::indicator { width: 13px; height: 13px; }");
        setFixedSize(720, 576);
        setAutoFillBackground(true);
        auto *navigation = new QListWidget(this);
        navigation->setGeometry(10, 10, 164, 510);
        navigation->addItems({"General", "NZB", "Servers"});
        navigation->setStyleSheet("QListWidget { border: 1px solid #828790; background: white; padding: 1px; }"
                                  "QListWidget::item { height: 15px; padding-left: 1px; }"
                                  "QListWidget::item:selected { background: #0078d4; color: white; }");
        auto *pages = new QStackedWidget(this);
        pages->setGeometry(184, 10, 526, 510);
        for (int i = 0; i < navigation->count(); ++i) {
            auto *page = new QWidget;
            pages->addWidget(page);
            auto *heading = caption(page, navigation->item(i)->text(), {4, 0, 485, 22});
            QFont bold = heading->font();
            bold.setBold(true);
            heading->setFont(bold);
            line(page, {4, 27, 518, 1});
        }

        auto *general = pages->widget(0);
        auto *startup = new QGroupBox("Startup and window behavior", general);
        startup->setGeometry(4, 42, 518, 128);
        addCheck(startup, "Connect automatically at startup", {13, 24, 480, 22}, true);
        addCheck(startup, "Minimize to the notification area", {13, 52, 480, 22}, true);
        addCheck(startup, "Close to the notification area", {13, 80, 480, 22}, false);
        caption(general, "Queue autosave interval", {4, 193, 174, 20});
        addEdit(general, "30", {183, 189, 66, 24});
        caption(general, "seconds", {259, 193, 104, 20});

        auto *nzb = pages->widget(1);
        caption(nzb, "Download folder", {4, 55, 130, 19});
        addEdit(nzb, R"(D:\Downloads)", {146, 51, 370, 24});
        caption(nzb, "NZB watch folder", {4, 88, 130, 19});
        addEdit(nzb, R"(D:\NZB)", {146, 84, 370, 24});
        addCheck(nzb, "Automatically import NZBs from the watch folder", {4, 126, 485, 22}, false);
        addCheck(nzb, "Add new collections paused", {4, 153, 485, 22}, false);

        auto *server = pages->widget(2);
        const QStringList actions = {"Add server", "Edit server", "Delete server", "Move up", "Move down"};
        for (int i = 0; i < actions.size(); ++i)
            push(server, actions[i], {4 + i * 104, 43, 98, 25})->setEnabled(false);
        auto *servers = new Grid({{"Name", 125}, {"Address", 148}, {"Port", 50, true},
                                 {"Conn.", 48, true}, {"SSL", 43}, {"Level", 57, true}}, server);
        servers->setObjectName("servers");
        servers->setGeometry(4, 78, 518, 235);
        servers->setGrid(false);
        servers->addRow({"Example Usenet", "news.example.invalid", "563", "4", "Yes", "0"});
        servers->addRow({"Backup", "backup.example.invalid", "563", "2", "Yes", "1"});
        servers->item(0, 0)->setCheckState(Qt::Checked);
        servers->item(1, 0)->setCheckState(Qt::Unchecked);
        connect(servers, &QTableWidget::cellChanged, this, [this] { markChanged(); });
        caption(server, "Tick a server to use it. Level 0 servers are used first.", {4, 324, 518, 22});
        connect(navigation, &QListWidget::currentRowChanged, pages, &QStackedWidget::setCurrentIndex);
        navigation->setCurrentRow(2);

        line(this, {10, 530, 700, 1});
        auto *ok = push(this, "OK", {454, 541, 80, 26});
        auto *cancel = push(this, "Cancel", {542, 541, 80, 26});
        apply = push(this, "Apply", {630, 541, 80, 26});
        apply->setEnabled(false);
        ok->setDefault(true);
        ok->setFocus();
        connect(ok, &QPushButton::clicked, this, &QDialog::accept);
        connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
        connect(apply, &QPushButton::clicked, this, [this] { apply->setEnabled(false); });
    }

private:
    void markChanged()
    {
        if (apply)
            apply->setEnabled(true);
    }

    void addCheck(QWidget *parent, const QString &text, QRect geometry, bool enabled)
    {
        auto *control = new QCheckBox(text, parent);
        control->setGeometry(geometry);
        control->setChecked(enabled);
        connect(control, &QCheckBox::toggled, this, [this] { markChanged(); });
    }

    void addEdit(QWidget *parent, const QString &value, QRect geometry)
    {
        auto *control = new QLineEdit(value, parent);
        control->setGeometry(geometry);
        connect(control, &QLineEdit::textChanged, this, [this] { markChanged(); });
    }

    QPushButton *apply = nullptr;
};

class Window final : public QWidget
{
public:
    Window()
    {
        setWindowTitle("NewsQueue — Qt 6 Mockup");
        setWindowIcon(icon("newsqueue-32", 32));
        setMinimumSize(650, 400);
        setFocusPolicy(Qt::StrongFocus);
        resize(1269, 791);

        auto *menu = new QMenuBar(this);
        menu->setNativeMenuBar(false);
        menu->setStyle(new MenuStyle(menu));
        menu->setStyleSheet("QMenuBar { background: white; spacing: 0; padding: 0; }"
                            "QMenuBar::item { padding: 0 7px; }"
                            "QMenuBar::item:selected { background: #d7ebff; }");
        fileMenu = menu->addMenu("&File");
        queueMenu = menu->addMenu("&Queue");
        viewMenu = menu->addMenu("&View");
        toolsMenu = menu->addMenu("&Tools");
        helpMenu = menu->addMenu("&Help");
        menuBar = menu;
        addCommand(fileMenu, "&Import NZB…", [this] { importSample(); }, QKeySequence::Open);
        fileMenu->addSeparator();
        addCommand(fileMenu, "E&xit", [this] { close(); });
        addCommand(queueMenu, "&Resume", [this] { setPaused(false); });
        addCommand(queueMenu, "&Pause", [this] { setPaused(true); });
        addCommand(queueMenu, "&Cleanup", [this] { cleanup(); });
        queueMenu->addSeparator();
        addCommand(queueMenu, "Collapse all", [this] { setExpanded(false); });
        addCommand(queueMenu, "Expand all", [this] { setExpanded(true); });
        connectionAction = viewMenu->addAction("Show &connections");
        connectionAction->setCheckable(true);
        connectionAction->setChecked(true);
        connectionAction->setShortcut(QKeySequence(Qt::Key_F9));
        connect(connectionAction, &QAction::toggled, this, [this] { arrange(); });
        viewMenu->addSeparator();
        auto *density = new QActionGroup(this);
        roomyAction = density->addAction(viewMenu->addAction("&Roomy"));
        compactAction = density->addAction(viewMenu->addAction("&Compact"));
        roomyAction->setCheckable(true);
        compactAction->setCheckable(true);
        compactAction->setChecked(true);
        connect(density, &QActionGroup::triggered, this, [this](QAction *selected) {
            setCompact(selected == compactAction);
        });
        addCommand(toolsMenu, "&Connect / disconnect", [this] { toggleConnection(); });
        addCommand(toolsMenu, "Set&up…", [this] { openSetup(); });
        addCommand(helpMenu, "&About NewsQueue", [this] { about(); });

        toolbar = new QWidget(this);
        toolbar->setAutoFillBackground(true);
        line(toolbar, {0, 0, 5000, 1});
        auto *toolbarHighlight = new QFrame(toolbar);
        toolbarHighlight->setGeometry(0, 1, 5000, 1);
        toolbarHighlight->setStyleSheet("background: white;");
        addToolbar(2);
        tabs = new TabBar(this);
        content = new QWidget(this);
        content->setStyleSheet("background: #f9f9f9; border: 1px solid #e5e5e5;");
        actionBar = new QWidget(this);
        actionBar->setAutoFillBackground(true);
        createActions();

        tables = {
            new Grid({{"Subject", 342}, {"Size", 88, true}, {"Parts", 80, true}, {"Age", 66},
                      {"Group", 153}, {"Poster", 128}, {"Status", 123}, {"Download folder", 205},
                      {"Unrar", 56}, {"Execute", 75}, {"Delete archives", 110}, {"Posted on", 112}}, this),
            new Grid({{"Name", 372}, {"Size", 120, true}, {"Blocks", 80, true}, {"Info", 225},
                      {"Unrar", 90}, {"Status", 150}}, this),
            new Grid({{"Time", 92}, {"Area", 125}, {"Message", 900}}, this),
            new Grid({{"Name", 440}, {"Date", 150}, {"Description", 570}}, this)
        };
        tables[0]->setObjectName("queue");
        tables[1]->addRow({"Public Domain Shorts – Vol. 2", "3.88 MB", "67", "All files present", "No", "Verified"});
        tables[2]->addRow({"14:27:12", "Download", "Started Open Skies – Episode 04."});
        tables[2]->addRow({"14:26:58", "PAR2", "Verified Public Domain Shorts – Vol. 2."});
        tables[2]->addRow({"14:26:41", "Connection", "Connected to Example Usenet (4 connections)."});
        tables[3]->addRow({"City Walks – Copenhagen", "2026-11-20", "New video in Example Feed"});
        tables[3]->addRow({"Night Signals – Live Session", "2026-11-19", "New audio in Example Feed"});

        connections = new Grid({{"#", 36}, {"Action", 110}, {"Description", 405}, {"Status", 150}, {"Server", 200}}, this);
        status = new QStatusBar(this);
        status->setStyleSheet("QStatusBar { background: #f0f0f0; border-top: 1px solid #d7d7d7; }");
        const int starts[] = {0, 110, 215, 345, 471, 600, 722, 850};
        const int widths[] = {110, 105, 130, 126, 129, 122, 128, 110};
        for (int i = 0; i < static_cast<int>(statusFields.size()); ++i) {
            if (i > 0) {
                auto *separator = new QFrame(status);
                separator->setGeometry(starts[i] - 1, 3, 1, 18);
                separator->setStyleSheet("background: #d7d7d7;");
            }
            statusFields[i] = caption(status, "", {starts[i] + 3, 2, widths[i] - 4, 20});
        }
        connect(tabs, &QTabBar::currentChanged, this, [this] { arrange(); });
        connect(filter, &QLineEdit::textChanged, this, [this] { fillQueue(); });
        connect(tables[0], &QTableWidget::cellClicked, this, [this](int row, int column) {
            Grid *table = tables[0];
            if (column == 0 && table->item(row, 0)->text().startsWith("Open Skies")) {
                const QPoint pointer = table->viewport()->mapFromGlobal(QCursor::pos());
                if (pointer.x() < table->columnViewportPosition(0) + 21)
                    setExpanded(!expanded);
            }
        });
        connect(tables[0], &QTableWidget::cellDoubleClicked, this, [this](int row, int column) {
            if (column == 0 && tables[0]->item(row, 0)->text().startsWith("Open Skies")) {
                if (tables[0]->viewport()->mapFromGlobal(QCursor::pos()).x() >= 21)
                    setExpanded(!expanded);
            }
        });
        auto *expandKey = new QShortcut(QKeySequence(Qt::Key_Right), tables[0]);
        auto *collapseKey = new QShortcut(QKeySequence(Qt::Key_Left), tables[0]);
        for (auto *shortcut : {expandKey, collapseKey})
            shortcut->setContext(Qt::WidgetShortcut);
        connect(expandKey, &QShortcut::activated, this, [this] { expandCurrent(true); });
        connect(collapseKey, &QShortcut::activated, this, [this] { expandCurrent(false); });
        refresh();
        arrange();
    }

    Setup *openSetup()
    {
        if (setup) {
            setup->raise();
            return setup;
        }
        setup = new Setup(this);
        setup->setAttribute(Qt::WA_DeleteOnClose);
        setup->show();
        return setup;
    }

    void setCompact(bool value)
    {
        compact = value;
        compactAction->setChecked(compact);
        roomyAction->setChecked(!compact);
        for (auto *table : tables)
            table->setGrid(compact);
        connections->setGrid(compact);
        arrange();
    }

    bool smoke()
    {
        auto *queue = tables[0];
        const bool loaded = queue->rowCount() == 9 && queue->item(0, 6)->data(progressRole).toInt() == 42;
        collapseButton->click();
        const bool collapsed = queue->rowCount() == 4;
        expandButton->click();
        const bool expandedAgain = queue->rowCount() == 9;
        filter->setText("copenhagen");
        const bool filtered = queue->rowCount() == 1 && queue->item(0, 0)->text().contains("Copenhagen");
        filter->clear();
        pauseButton->click();
        const bool pausedNow = queue->item(0, 6)->data(progressRole).toInt() == -42;
        connectButton->click();
        const bool disconnected = statusFields[0]->text() == "Disconnected";
        connectButton->click();
        pauseButton->click();
        tabs->setCurrentIndex(1);
        const bool par2 = tables[1]->isVisible() && queue->isHidden() && connections->isHidden();
        tabs->setCurrentIndex(2);
        const bool log = tables[2]->isVisible();
        tabs->setCurrentIndex(3);
        const bool rss = tables[3]->isVisible();
        tabs->setCurrentIndex(0);
        connectionAction->trigger();
        const bool panelHidden = connections->isHidden();
        connectionAction->trigger();
        roomyAction->trigger();
        const bool roomy = !compact && !queue->item(0, 0)->data(gridRole).toBool();
        compactAction->trigger();
        importSample();
        const bool added = queue->rowCount() == 10;
        cleanup();
        const bool cleanedNow = queue->rowCount() == 9;
        auto *dialog = openSetup();
        auto *pages = dialog->findChild<QListWidget *>();
        auto *servers = dialog->findChild<QTableWidget *>("servers");
        pages->setCurrentRow(0);
        const bool general = servers->isHidden() || !servers->isVisible();
        pages->setCurrentRow(1);
        bool nzb = false;
        for (auto *edit : dialog->findChildren<QLineEdit *>())
            if (edit->isVisible() && edit->text() == R"(D:\NZB)")
                nzb = true;
        pages->setCurrentRow(2);
        const bool serverRows = servers->isVisible() && servers->rowCount() == 2;
        dialog->reject();
        return loaded && collapsed && expandedAgain && filtered && pausedNow && disconnected &&
            par2 && log && rss && panelHidden && roomy && added && cleanedNow &&
            general && nzb && serverRows && connections->isVisible();
    }

protected:
    void resizeEvent(QResizeEvent *) override
    {
        if (status)
            arrange();
    }

private:
    template<typename Handler>
    void addCommand(QMenu *menu, const QString &title, Handler run, const QKeySequence &key = {})
    {
        auto *action = menu->addAction(title);
        action->setShortcut(key);
        connect(action, &QAction::triggered, this, run);
    }

    void addToolbar(int y)
    {
        struct Entry { int x, width; const char *label, *name; bool enabled; };
        const Entry entries[] = {
            {0, 56, "Connect", "connect", true}, {62, 45, "Pause", "pause", true},
            {108, 56, "Pause for", "pausefor", false}, {171, 41, "NZB", "nzb", true},
            {218, 67, "Download", "download", false}, {298, 43, "Cleanup", "cleanup", true},
            {353, 65, "Scheduler", "scheduler", false}, {427, 62, "Shutdown", "shutdown", false},
            {495, 48, "Setup", "setup", true}, {548, 45, "Home", "home", false},
            {594, 50, "Version?", "version", false}, {647, 42, "About", "about", true},
            {691, 40, "Notes", "notes", false}, {731, 41, "Stats", "stats", false},
            {773, 40, "To tray", "tray", true},
        };
        const int separators[] = {58, 167, 214, 293, 349, 420, 493, 542};
        for (int x : separators) {
            auto *separator = new QFrame(toolbar);
            separator->move(x, 4);
            separator->setFixedSize(1, 42);
            separator->setStyleSheet("background: #898989;");
            toolSeparators.append(separator);
        }
        for (const auto &entry : entries) {
            auto *button = new ToolButton(entry.label, entry.name, true, toolbar);
            button->setGeometry(entry.x, y, entry.width, 46);
            button->setEnabled(entry.enabled);
            mainButtons.append(button);
            const QString name = entry.name;
            if (name == "connect") {
                connectButton = button;
                button->setCheckable(true);
                connect(button, &QToolButton::clicked, this, [this] { toggleConnection(); });
            } else if (name == "pause") {
                pauseButton = button;
                button->setCheckable(true);
                connect(button, &QToolButton::clicked, this, [this] { setPaused(!paused); });
            } else if (name == "nzb") {
                connect(button, &QToolButton::clicked, this, [this] { importSample(); });
            } else if (name == "cleanup") {
                connect(button, &QToolButton::clicked, this, [this] { cleanup(); });
            } else if (name == "setup") {
                connect(button, &QToolButton::clicked, this, [this] { openSetup(); });
            } else if (name == "about") {
                connect(button, &QToolButton::clicked, this, [this] { about(); });
            } else if (name == "tray") {
                connect(button, &QToolButton::clicked, this, &QWidget::showMinimized);
            }
        }
    }

    void createActions()
    {
        queueActions = new QWidget(actionBar);
        const QStringList names = {"play", "pause_s", "top", "up", "down", "bottom", "collapse", "expand",
                                   "unrar", "nounrar", "folder", "rename", "export", "folder", "key", "eta"};
        const QStringList labels = {"Resume", "Pause", "Move to top", "Move up", "Move down", "Move to bottom",
                                    "Collapse all", "Expand all", "Unpack", "Do not unpack", "Change unpack folder",
                                    "Rename", "Export", "Open folder", "Archive password", "Estimated time left"};
        const int starts[] = {0, 23, 54, 77, 100, 123, 154, 177, 208, 231, 254, 285, 308, 331, 362, 385};
        for (int x : {48, 148, 202, 279, 356})
            line(queueActions, {x, 2, 1, 18});
        for (int i = 0; i < names.size(); ++i) {
            auto *button = new ToolButton(labels[i], names[i], false, queueActions);
            button->setGeometry(starts[i], 0, 23, 22);
            button->setEnabled(i < 2 || i == 6 || i == 7);
            if (i == 0)
                connect(button, &QToolButton::clicked, this, [this] { setPaused(false); });
            if (i == 1)
                connect(button, &QToolButton::clicked, this, [this] { setPaused(true); });
            if (i == 6) {
                collapseButton = button;
                connect(button, &QToolButton::clicked, this, [this] { setExpanded(false); });
            }
            if (i == 7) {
                expandButton = button;
                connect(button, &QToolButton::clicked, this, [this] { setExpanded(true); });
            }
        }
        filterLabel = new QLabel("Filter", queueActions);
        filterLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        filter = new QLineEdit(queueActions);
        filter->setAccessibleName("Filter subjects");
        filterLabel->setBuddy(filter);

        filter->setPlaceholderText("Filter subjects");
        filter->setStyleSheet("QLineEdit { background: white; border: 1px solid #e4e4e4; border-bottom-color: #898989;"
                              "border-radius: 2px; padding: 0 1px; }");
        const QList<QStringList> labelsByTab = {{"Start", "Import…", "Remove"}, {"Clear", "Copy"},
                                                {"Refresh", "Feeds…", "Filter help"}};
        for (int tab = 0; tab < labelsByTab.size(); ++tab) {
            tabActions[tab] = new QWidget(actionBar);
            for (int i = 0; i < labelsByTab[tab].size(); ++i) {
                auto *button = push(tabActions[tab], labelsByTab[tab][i], {5 + 88 * i, 3, 82, 24});
                button->setEnabled(false);
                if (tab == 1 && i == 0) {
                    button->setEnabled(true);
                    connect(button, &QPushButton::clicked, this, [this] { tables[2]->setRowCount(0); });
                }
            }
        }
        auto *timer = new QCheckBox("Refresh on a timer", tabActions[2]);
        timer->setGeometry(269, 3, 155, 24);
    }
    void arrange()
    {
        const int w = width();
        const int h = height();
        constexpr int menuHeight = 20;
        constexpr int statusHeight = 23;
        const int toolbarHeight = compact ? 50 : 58;
        const int tabY = menuHeight + toolbarHeight;
        const int contentY = tabY + 23;
        const int actionsY = contentY + 1;
        const int bottom = h - statusHeight - 4;
        const int tableY = actionsY + 30;
        const int pageWidth = w - 8;
        const int currentTab = tabs->currentIndex();
        const bool showConnections = currentTab == 0 && connectionAction->isChecked();
        const int panelHeight = showConnections ? qBound(84, (bottom - actionsY) / 4, 158) : 0;

        menuBar->setGeometry(0, 0, w, menuHeight);
        toolbar->setGeometry(0, menuHeight, w, toolbarHeight);
        for (auto *button : mainButtons)
            button->resize(button->width(), compact ? 46 : 54);
        for (auto *separator : toolSeparators)
            separator->resize(1, compact ? 42 : 50);
        tabs->setGeometry(0, tabY, w, 23);
        content->setGeometry(0, contentY, w, h - statusHeight - contentY);
        actionBar->setGeometry(4, actionsY, pageWidth, 30);
        queueActions->setGeometry(0, 4, pageWidth, 23);
        queueActions->setVisible(currentTab == 0);
        filterLabel->setGeometry(pageWidth - 233, -1, 40, 22);
        filter->setGeometry(pageWidth - 187, 0, 183, 22);
        for (int i = 0; i < static_cast<int>(tabActions.size()); ++i) {
            tabActions[i]->setGeometry(0, 0, pageWidth, 30);
            tabActions[i]->setVisible(currentTab == i + 1);
        }
        for (int i = 0; i < static_cast<int>(tables.size()); ++i)
            tables[i]->setVisible(currentTab == i);
        tables[currentTab]->setGeometry(4, tableY, pageWidth,
                                        bottom - tableY - panelHeight - (showConnections ? 5 : 0));
        connections->setVisible(showConnections);
        connections->setGeometry(4, bottom - panelHeight, pageWidth, panelHeight);
        status->setGeometry(0, h - statusHeight, w, statusHeight);
    }

    void toggleConnection()
    {
        connected = !connected;
        refresh();
    }

    void setPaused(bool value)
    {
        paused = value;
        refresh();
    }

    void setExpanded(bool value)
    {
        if (expanded == value)
            return;
        expanded = value;
        fillQueue();
    }

    void expandCurrent(bool value)
    {
        const auto *subject = tables[0]->item(tables[0]->currentRow(), 0);
        if (subject && subject->text().startsWith("Open Skies")) {
            setExpanded(value);
            tables[0]->setCurrentCell(0, 0);
        }
    }

    void cleanup()
    {
        cleaned = true;
        fillQueue();
    }

    void importSample()
    {
        imported = true;
        tables[0]->horizontalScrollBar()->setValue(0);
        tabs->setCurrentIndex(0);
        fillQueue();
    }

    void about()
    {
        QMessageBox::information(this, "About NewsQueue", "NewsQueue 1.0.7\nQt 6 desktop mockup");
    }

    void fillQueue()
    {
        auto *queue = tables[0];
        queue->setUpdatesEnabled(false);
        queue->setRowCount(0);
        const auto match = [this](const QString &subject) {
            return subject.contains(filter->text(), Qt::CaseInsensitive);
        };
        auto activeText = [this](int progress) -> QString {
            if (!connected && !paused)
                return "Waiting";
            return (paused ? QString("Paused %1%") : QString("Downloading %1%")).arg(progress);
        };
        auto progress = [this](int value) { return paused ? -value : (connected ? value : 0); };
        auto family = [queue](QStringList columns, const QString &image, int percent = 0) {
            if (columns.size() == 9)
                columns.append({"No", "No", "2026-11-18"});
            queue->addRow(columns, image, false, percent);
        };

        const QString openSkies = "Open Skies – Episode 04";
        if (match(openSkies)) {
            family({openSkies, "3.62 GB", "2,821", "2 d", "alt.binaries.hdtv", "Skyline",
                    activeText(42), R"(D:\Downloads\TV)", "Yes"}, expanded ? "minus" : "plus", progress(42));
            if (expanded) {
                const QStringList parts = {"368/368", "368/368", "229/368", "0/368"};
                const QStringList states = {"Completed", "Completed", activeText(62), "Queued"};
                for (int i = 0; i < parts.size(); ++i)
                    queue->addRow({QString("open-skies.s01e04.part0%1.rar").arg(i + 1), "476 MB", parts[i],
                                   "2 d", "alt.binaries.hdtv", "Skyline", states[i]},
                                  "file", true, i == 2 ? progress(62) : 0);
                queue->addRow({"open-skies.s01e04.par2", "1.24 MB", "2/2", "2 d",
                               "alt.binaries.hdtv", "Skyline", "Completed"}, "file", true);
            }
        }
        const QString copenhagen = "City Walks – Copenhagen";
        if (match(copenhagen))
            family({copenhagen, "1.48 GB", "1,104", "5 d", "alt.binaries.video",
                    "FieldNotes", "Queued", R"(D:\Downloads\Video)", "Yes"}, "plus");
        const QString nightSignals = "Night Signals – Live Session";
        if (match(nightSignals))
            family({nightSignals, "674 MB", "509", "16 d", "alt.binaries.sounds",
                    "Wavelength", "Paused 27%", R"(D:\Downloads\Music)", "Yes"}, "plus", -27);
        const QString shorts = "Public Domain Shorts – Vol. 2";
        if (!cleaned && match(shorts))
            family({shorts, "512 MB", "398", "21 d", "alt.binaries.video",
                    "Archive", "Completed", R"(D:\Downloads\Video)", "No"}, "plus");
        const QString sample = "Sample Collection";
        if (imported && match(sample))
            family({sample, "238 MB", "185", "1 d", "alt.binaries.test",
                    "Demo", "Queued", R"(D:\Downloads)", "Yes"}, "plus");
        queue->setUpdatesEnabled(true);
    }

    void refresh()
    {
        const bool running = connected && !paused;
        connectButton->setChecked(connected);
        pauseButton->setChecked(paused);
        const QStringList values = {connected ? "Connected" : "Disconnected", running ? "18.4 MB/s" : "0.0 MB/s",
                                    paused ? "Paused" : (connected ? "Download active" : "Idle"),
                                    "5.82 GB queued", "9.70 GB session", "24.1 GB today",
                                    "D: 621 GB free", "Demo data"};
        for (int i = 0; i < values.size(); ++i)
            statusFields[i]->setText(values[i]);
        connections->setRowCount(0);
        for (int i = 0; i < 4; ++i)
            connections->addRow({QString::number(i + 1), running ? "Download" : "Idle",
                                 connected ? QString("open-skies.s01e04.part0%1.rar").arg(i == 3 ? 4 : 3) : QString(),
                                 running ? (i < 2 ? "Receiving" : "Waiting") : "Idle", "Example Usenet"});
        fillQueue();
    }

    QWidget *toolbar = nullptr;
    QWidget *content = nullptr;
    QWidget *actionBar = nullptr;
    QWidget *queueActions = nullptr;
    QMenuBar *menuBar = nullptr;
    QMenu *fileMenu = nullptr;
    QMenu *queueMenu = nullptr;
    QMenu *viewMenu = nullptr;
    QMenu *toolsMenu = nullptr;
    QMenu *helpMenu = nullptr;
    ToolButton *connectButton = nullptr;
    ToolButton *pauseButton = nullptr;
    ToolButton *collapseButton = nullptr;
    ToolButton *expandButton = nullptr;
    QAction *connectionAction = nullptr;
    QAction *compactAction = nullptr;
    QAction *roomyAction = nullptr;
    QLineEdit *filter = nullptr;
    QLabel *filterLabel = nullptr;
    TabBar *tabs = nullptr;
    Grid *connections = nullptr;
    QStatusBar *status = nullptr;
    QPointer<Setup> setup;
    QList<ToolButton *> mainButtons;
    QList<QFrame *> toolSeparators;
    std::array<QWidget *, 3> tabActions{};
    std::array<Grid *, 4> tables{};
    std::array<QLabel *, 8> statusFields{};
    bool compact = true;
    bool connected = true;
    bool paused = false;
    bool expanded = true;
    bool cleaned = false;
    bool imported = false;
};

bool capture(QWidget &window, const QString &path)
{
    window.ensurePolished();
    QImage image(window.size(), QImage::Format_RGB32);
    image.fill(face);
    window.render(&image);
    return image.save(path, "PNG");
}

} // namespace

int main(int argc, char **argv)
{
    bool headless = false;
    for (int i = 1; i < argc; ++i) {
        const QByteArray argument(argv[i]);
        if (argument == "--smoke" || argument == "--capture")
            headless = true;
    }
    if (headless) {
        qputenv("QT_QPA_PLATFORM", "offscreen");
        qputenv("QT_SCALE_FACTOR", "1");
        if (qEnvironmentVariableIsSet("WINDIR"))
            qputenv("QT_QPA_FONTDIR", qgetenv("WINDIR") + "/Fonts");
    }
    QApplication app(argc, argv);
    QApplication::setApplicationName("NewsQueue Qt 6 Mockup");
    if (headless && qEnvironmentVariableIsSet("WINDIR")) {
        const QString fonts = qEnvironmentVariable("WINDIR") + "/Fonts/";
        QFontDatabase::addApplicationFont(fonts + "segoeui.ttf");
        QFontDatabase::addApplicationFont(fonts + "segoeuib.ttf");
    }
    QFont font("Segoe UI");
    font.setPixelSize(12);
    QApplication::setFont(font);
    QPalette palette = app.palette();
    palette.setColor(QPalette::Window, face);
    palette.setColor(QPalette::Base, paper);
    palette.setColor(QPalette::Button, face);
    palette.setColor(QPalette::WindowText, Qt::black);
    palette.setColor(QPalette::Text, Qt::black);
    palette.setColor(QPalette::ButtonText, Qt::black);
    palette.setColor(QPalette::PlaceholderText, QColor("#4c4c4c"));
    app.setPalette(palette);

    Window window;
    const QStringList args = app.arguments();
    window.setCompact(!args.contains("--roomy"));
    window.show();
    window.setFocus();
    if (!headless)
        return app.exec();

    app.processEvents();
    const int captureFlag = args.indexOf("--capture");
    if (args.contains("--smoke") && !window.smoke())
        return 1;
    if (captureFlag >= 0) {
        const QString output = args.value(captureFlag + 1);
        if (output.isEmpty() || !QDir().mkpath(output))
            return 2;
        QDir folder(output);
        if (!capture(window, folder.filePath("queue-compact-client.png")))
            return 3;
        auto *setup = window.openSetup();
        app.processEvents();
        if (!capture(*setup, folder.filePath("setup-client.png")))
            return 4;
    }
    return 0;
}
