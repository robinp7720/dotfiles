import QtQuick
import QtTest
import "../../qml" as TV

TestCase {
    name: "TVMotion"
    when: windowShown
    Component {
        id: motionComponent
        TV.Motion {
            duration: 400
            exitDuration: 300
        }
    }
    function test_reversal_preserves_current_position() {
        let motion = createTemporaryObject(motionComponent, this);
        compare(motion.progress, 0);
        motion.shown = true;
        tryVerify(() => motion.progress > 0 && motion.progress < 1);
        let before = motion.progress;
        motion.shown = false;
        compare(motion.progress, before);
        tryCompare(motion, "progress", 0);
        compare(motion.running, false);
    }
    function test_reduced_motion_finishes_and_stays_still() {
        let motion = createTemporaryObject(motionComponent, this);
        motion.shown = true;
        tryVerify(() => motion.progress > 0 && motion.progress < 1);
        motion.reducedMotion = true;
        compare(motion.progress, 1);
        compare(motion.running, false);
        motion.shown = false;
        compare(motion.progress, 0);
        motion.shown = true;
        motion.replay();
        compare(motion.progress, 1);
        compare(motion.running, false);
    }
}
