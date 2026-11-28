class PostTest {
    static int run(boolean active) {
        int total = 2;
        do {
            if (active) break;
            total = 9;
        } while (false);
        return total;
    }
}
