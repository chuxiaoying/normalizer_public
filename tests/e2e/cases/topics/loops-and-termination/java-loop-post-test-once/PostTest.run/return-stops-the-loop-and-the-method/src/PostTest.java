class PostTest {
    static int run(boolean active) {
        do {
            if (active) return 11;
        } while (false);
        return 7;
    }
}
