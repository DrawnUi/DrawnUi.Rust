# Sourced by run-sim.sh and run-device.sh: what an example is called on iOS. In: crate (the
# example's package, `--example`). Out: name (under the icon), bundle (the .app's file name),
# bundle_id (net.drawnui.<crate>), launch_color (the launch screen, RRGGBB, before the app runs;
# match the app's `Ui::background`, which the view behind the canvas takes from then on).
launch_color=000000
case "$crate" in
    hellorust) name=HelloRust; bundle=HelloRust ;;
    dungeon) name="Dungeon Run"; bundle=DungeonRun; launch_color=000000 ;;
    *) name="$crate"; bundle="$crate" ;;
esac
bundle_id="net.drawnui.$crate"
