class Counter { int position; }
class SmallLoop {
    static int total() {
        Counter counter = new Counter(); Counter alias = counter; int sum = 0;
        for (alias.position = 3; counter.position < 3; counter.position++) sum += counter.position;
        return sum + counter.position;
    }
}
