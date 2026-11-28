class PostTest {
    static int run(boolean active) {
        int total = 0;
        do {
            total++;
        } while (active);
        return total;
    }
}
