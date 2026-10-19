import QtQuick
import TvMode.Video 1.0

VideoSurface {
    signal ready(bool success)
    onInitialized: success => ready(success)
}
