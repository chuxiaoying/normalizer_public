class Guard {
    static boolean allowed(boolean active, int value) {
        return false && value > 0;
    }
}
