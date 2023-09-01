use std::cell::RefCell;
use std::ops::Add;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Condvar, Mutex};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};
use crate::data_structures::cache::Cache;
use crate::data_structures::delayed_queue::DelayedQueue;
use crate::data_structures::dispatcher::Dispatcher;
use crate::data_structures::execution_limiter::ExecutionLimiter;
use crate::data_structures::looper::Looper;
use crate::data_structures::mpmc_channel::MpMcChannel;
use crate::data_structures::ranking_barrier::RankingBarrier;

mod data_structures;

#[derive(Debug)]
struct Node {
    next: Option<Rc<RefCell<Node>>>,
    head: Option<Weak<RefCell<Node>>>,
}

impl Drop for Node {
    fn drop(&mut self) {
        println!("Dropping");
    }
}

fn run(msg: &str) { for i in 0..10 {
    println!("{}{}",msg,i);
    thread::sleep(Duration::from_nanos(1)); }
}

fn main() {

    // Esempio Rc e Weak

    let a = Rc::new(RefCell::new(Node {next: None, head: None}));
    println!("a strong count: {:?}, weak count: {:?}", Rc::strong_count(&a), Rc::weak_count(&a));
    let b = Rc::new(RefCell::new(Node {next: Some(Rc::clone(&a)), head: None}));
    println!("a strong count: {:?}, weak count: {:?}", Rc::strong_count(&a), Rc::weak_count(&a));
    println!("b strong count: {:?}, weak count: {:?}", Rc::strong_count(&b), Rc::weak_count(&b));
    let c = Rc::new(RefCell::new(Node {next: Some(Rc::clone(&b)), head: None}));

    // Creates a reference cycle with weak
    // (*a).borrow_mut().head = Some(Rc::downgrade(&c));
    // Creates a reference cycle
    (*a).borrow_mut().next = Some(Rc::clone(&c));
    println!("a strong count: {:?}, weak count: {:?}", Rc::strong_count(&a), Rc::weak_count(&a));
    println!("b strong count: {:?}, weak count: {:?}", Rc::strong_count(&b), Rc::weak_count(&b));
    println!("c strong count: {:?}, weak count: {:?}", Rc::strong_count(&c), Rc::weak_count(&c));

    drop(a);
    drop(b);
    drop(c);

    println!("Debug point");

    // Esempio condition variables

    let pair = Arc::new((Mutex::new(false), Condvar::new()));
    let pair2 = Arc::clone(&pair);

    thread::spawn(move || {
        println!("Going to sleep");
        thread::sleep(Duration::from_secs(2));
        println!("I'm awake!");
        let (m2, c2) = &*pair2;
        let mut started = m2.lock().unwrap();
        *started = true;
        c2.notify_one();
    });

    let (m1, c1) = &*pair;
    let mut started = m1.lock().unwrap();
    println!("I want to start :(");
    while !*started {
        started = c1.wait(started).unwrap();
    }
    println!("Started :)");

    // esempio canali

    use std::sync::mpsc::sync_channel; use std::thread;
    let (tx, rx) = channel();
    for _ in 0..3 {
        let tx = tx.clone();
    }
    println!("{}", msg);

    // Esempio di non determinismo, interferenza e data race

    let t1 = thread::spawn(|| {run( "aaaa"); });
    let t2 = thread::spawn(|| {run(" bbbb"); });
    t1.join().unwrap();
    t2.join().unwrap();

    // Esempio delayed_queue.rs

    let delayed_queue = DelayedQueue::new();
    let delayed_queue2 = Arc::clone(&delayed_queue);
    let delayed_queue3 = Arc::clone(&delayed_queue);
    let delayed_queue4 = Arc::clone(&delayed_queue);
    let delayed_queue5 = Arc::clone(&delayed_queue);

    thread::spawn(move || {
        delayed_queue.offer(20, Instant::now().add(Duration::from_secs(5)));
        delayed_queue.offer(30, Instant::now().add(Duration::from_secs(10)));
        delayed_queue.offer(10, Instant::now().add(Duration::from_secs(15)));
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        delayed_queue2.take();
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        delayed_queue3.take();
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        delayed_queue4.take();
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        delayed_queue5.take();
    });

    // Esempio mpmc_channel.rs

    let mpmc_channel = MpMcChannel::new(3);
    let mpmc_channel2 = Arc::clone(&mpmc_channel);
    let mpmc_channel3 = Arc::clone(&mpmc_channel);

    thread::spawn(move || {
        mpmc_channel.send(10);
        mpmc_channel.send(20);
        mpmc_channel.send(20);
        mpmc_channel.send(20);
        thread::sleep(Duration::from_secs(10));
        mpmc_channel.send(20);
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        mpmc_channel2.recv();
        mpmc_channel2.recv();
        mpmc_channel2.recv();
        mpmc_channel2.recv();
        mpmc_channel2.recv();
        thread::sleep(Duration::from_secs(10));
        mpmc_channel2.recv();
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(8));
        mpmc_channel3.shutdown();
    });


    // Esempio looper.rs

    let looper = Looper::new(|a| {}, || {});
    let looper2 = Arc::clone(&looper);
    let looper3 = Arc::clone(&looper);

    thread::spawn(move || {
        looper2.send(1);
        thread::sleep(Duration::from_secs(2));
        looper2.send(2);
    }).join().unwrap();

    thread::spawn(move || {
        looper3.send(3);
        looper3.send(4);
    }).join().unwrap();

    // Esempio execution_limiter.rs

    let execution_limiter = ExecutionLimiter::new(2);
    let execution_limiter2 = Arc::clone(&execution_limiter);
    let execution_limiter3 = Arc::clone(&execution_limiter);

    thread::spawn(move || {
        execution_limiter.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter.execute(|| {
            for i in 0..10000000 {}
        });
    });

    thread::spawn(move || {
        execution_limiter2.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter2.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter2.execute(|| {
            for i in 0..10000000 {}
        });
    });

    thread::spawn(move || {
        execution_limiter3.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter3.execute(|| {
            for i in 0..10000000 {}
        });
        execution_limiter3.execute(|| {
            for i in 0..10000000 {}
        });
    });

    // Esempio di ranking_barrier.rs

    let rb = RankingBarrier::new(3);
    let rb2 = Arc::clone(&rb);
    let rb3 = Arc::clone(&rb);
    let rb4 = Arc::clone(&rb);
    let rb5 = Arc::clone(&rb);
    let rb6 = Arc::clone(&rb);

    thread::spawn(move || {
        rb.wait();
    });

    thread::spawn(move || {
        rb2.wait();
    });

    thread::spawn(move || {
        rb3.wait();
    });

    thread::spawn(move || {
        rb4.wait();
    });

    thread::spawn(move || {
        rb5.wait();
    });

    thread::spawn(move || {
        rb6.wait();
    });

    // Esempio dispatcher.rs

    let dispatcher = Dispatcher::new();
    let dispatcher2 = Arc::clone(&dispatcher);

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        println!("INVIO");
        dispatcher.dispatch(10);
        dispatcher.dispatch(11);
        dispatcher.dispatch(12);
        drop(dispatcher);
    });

    thread::spawn(move || {
        let subscription = dispatcher2.subscribe();
        drop(dispatcher2);
        thread::sleep(Duration::from_secs(4));
        for i in 0..5 {
            subscription.read();
        }
    });

    // Esempio cache.rs real

    let cache = Cache::new();
    let cache2 = Arc::clone(&cache);
    let cache3 = Arc::clone(&cache);
    let cache4 = Arc::clone(&cache);
    let cache5 = Arc::clone(&cache);

    thread::spawn(move || {
        cache.get(10, |k| {
            println!("Calcolo");
            thread::sleep(Duration::from_secs(3));
            return k*2;
        });
    });

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        cache2.get(10, |k| {
            return k*3;
        });
    });

}