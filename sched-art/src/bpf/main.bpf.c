#include <scx/common.bpf.h>
#include <vmlinux.h>

char _license[] SEC("license") = "GPL";

UEI_DEFINE(uei);

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __type(key, pid_t);
    __type(value, u8);
    __uint(max_entries, 1024);
} prior_tasks SEC(".maps");

static bool is_prior_task(pid_t pid)
{
    if (bpf_map_lookup_elem(&prior_tasks, &pid) == NULL) {
        return false;
    } else {
        return true;
    }
}

s32 BPF_STRUCT_OPS(art_select_cpu, struct task_struct *p, s32 prev_cpu, u64 wake_flags)
{
    if (is_prior_task(p->pid)) {
        scx_bpf_dsq_insert(p, SCX_DSQ_LOCAL_ON | prev_cpu, SCX_SLICE_DFL, SCX_ENQ_HEAD);
        return prev_cpu; // This value is not used
    }

    if (p->nr_cpus_allowed == 1 ||
        scx_bpf_test_and_clear_cpu_idle(prev_cpu))
        return prev_cpu;

    s32 cpu = scx_bpf_pick_idle_cpu(p->cpus_ptr, 0);
    if (cpu >= 0)
        return cpu;

    return prev_cpu;
}

void BPF_STRUCT_OPS(art_enqueue, struct task_struct *p, u64 enq_flags)
{
    if (is_prior_task(p->pid)) {
        scx_bpf_dsq_insert(p, SCX_DSQ_LOCAL, SCX_SLICE_DFL, SCX_ENQ_HEAD);
    }

    scx_bpf_dsq_insert(p, SCX_DSQ_LOCAL, SCX_SLICE_DFL, 0);

}

void BPF_STRUCT_OPS(art_dispatch, s32 cpu, struct task_struct *prev)
{
    bpf_printk("art_dispatch seems not to be called");
}

s32 BPF_STRUCT_OPS_SLEEPABLE(art_init)
{
    return 0;
}

void BPF_STRUCT_OPS(art_exit, struct scx_exit_info *ei)
{
    UEI_RECORD(uei, ei);
}

SCX_OPS_DEFINE(art_ops,
                  .select_cpu      = (void *)art_select_cpu,
                  .enqueue         = (void *)art_enqueue,
                  .dispatch        = (void *)art_dispatch,
                  .init            = (void *)art_init,
                  .exit            = (void *)art_exit,
                  .name            = "art"
              );
