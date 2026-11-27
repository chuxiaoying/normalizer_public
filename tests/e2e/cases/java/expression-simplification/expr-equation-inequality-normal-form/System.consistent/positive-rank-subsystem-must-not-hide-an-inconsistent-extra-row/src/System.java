class System {
    static boolean consistent(int value, int target, int limit) {
        return 2 * value + target == 11 && value - target == 1 && value + target == 0;
    }
}
