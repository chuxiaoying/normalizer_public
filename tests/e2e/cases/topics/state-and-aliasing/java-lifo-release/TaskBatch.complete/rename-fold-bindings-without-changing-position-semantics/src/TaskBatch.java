record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        /* @normalizer-ir
           stack = fold opened from [] as (running, member) yield value.push(running, member)
        */
        /* @normalizer-ir
           stack = fold closed from stack as (running, member)
             when value.length(running) == 0 throw IllegalArgumentException
             when value.last_or(running, member).id != member.id throw IllegalArgumentException
             yield value.pop(running)
        */
        boolean empty;
        /* @normalizer-ir empty = value.length(stack) == 0 */
        return empty;
    }
}
