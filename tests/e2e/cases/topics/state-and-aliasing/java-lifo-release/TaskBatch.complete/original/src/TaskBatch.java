record Task(int id) {}
class TaskBatch {
    static boolean complete(Task[] opened, Task[] closed) {
        /* @normalizer-ir
           stack = fold opened from [] as (s, task) yield value.push(s, task)
        */
        /* @normalizer-ir
           stack = fold closed from stack as (s, task)
             when value.length(s) == 0 throw IllegalArgumentException
             when value.last_or(s, task).id != task.id throw IllegalArgumentException
             yield value.pop(s)
        */
        boolean empty;
        /* @normalizer-ir empty = value.length(stack) == 0 */
        return empty;
    }
}
