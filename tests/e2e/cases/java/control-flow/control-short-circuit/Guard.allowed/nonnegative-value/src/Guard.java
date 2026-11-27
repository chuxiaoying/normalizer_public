class Guard {
    static boolean allowed(boolean active, int value) {
        return active && value >= 0;
    }
}
