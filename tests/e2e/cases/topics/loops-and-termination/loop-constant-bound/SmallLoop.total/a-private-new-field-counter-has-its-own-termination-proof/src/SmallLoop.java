class Counter { int position; }
class SmallLoop {
    static int total() {
        Counter counter = new Counter(); Counter alias = counter; int sum = 0;
        for (counter.position = 0; counter.position < 3; alias.position++) sum += counter.position;
        return sum;
    }
}
