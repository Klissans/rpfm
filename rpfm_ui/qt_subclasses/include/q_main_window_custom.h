#ifndef QMAINWINDOWCUSTOM_H
#define QMAINWINDOWCUSTOM_H

#include <QMainWindow>
#include <QCloseEvent>
#include <QMoveEvent>
#include <QEvent>
#include <QMessageBox>
#include <QSettings>
#include <KBusyIndicatorWidget>

extern "C" QMainWindow* new_q_main_window_custom(bool (*are_you_sure)(QMainWindow* main_window, bool is_delete_my_mod) = nullptr, bool is_dark_theme_enabled = false);

class QMainWindowCustom : public QMainWindow
{
    Q_OBJECT
public:
    explicit QMainWindowCustom(QWidget *parent = nullptr, bool (*are_you_sure)(QMainWindow* main_window, bool is_delete_my_mod) = nullptr, bool is_dark_theme_enabled = false);
    void closeEvent(QCloseEvent *event) override;
    void moveEvent(QMoveEvent *event) override;
    void changeEvent(QEvent *event) override;

private:
    bool (*are_you_sure)(QMainWindow* main_window, bool is_delete_my_mod);
    bool dark_theme_enabled;
    KBusyIndicatorWidget* busyIndicator;

protected:
    void dragEnterEvent(QDragEnterEvent *event) override;
    void dragMoveEvent(QDragMoveEvent *event) override;
    void dragLeaveEvent(QDragLeaveEvent *event) override;
    void dropEvent(QDropEvent *event) override;

signals:
    void openPack(QStringList const &);

};

#endif // QMAINWINDOWCUSTOM_H
