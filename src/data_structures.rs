pub mod looper {
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::sync::mpsc::{RecvError, sync_channel, SyncSender};
    use std::thread;

    pub struct Looper<Message: Send + 'static + Clone + Debug> {
        sender: SyncSender<Message>
    }

    impl<Message: Send + 'static + Clone + Debug> Looper<Message>{
        pub fn new(
            process: impl Fn(Message) + Send + 'static,
            cleanup: impl FnOnce() + Send +'static
        ) -> Arc<Self> {
            let (tx,rx) = sync_channel(0);
            thread::spawn(move || {
                loop {
                    match rx.recv() {
                        Ok(msg) => {
                            println!("Processing: {:?}", msg);
                            process(msg);
                        }
                        Err(_) => {
                            println!("Cleaning up");
                            cleanup();
                            break;
                        }
                    }
                }
            });
            Arc::new( Looper { sender: tx } )
        }

        pub fn send(&self, msg: Message) {
            self.sender.send(msg).unwrap();
        }
    }
}

pub mod execution_limiter {
    use std::sync::{Arc, Condvar, Mutex};

    pub struct Dropper<'a> {
        element: &'a ExecutionLimiter
    }

    impl<'a> Drop for Dropper<'a> {
        fn drop(&mut self) {
            println!("Decremento");
            (*self.element.count.lock().unwrap()) -= 1;
            self.element.condvar.notify_all();
        }
    }

    pub struct ExecutionLimiter {
        count: Mutex<usize>,
        condvar: Condvar,
        limit: usize
    }

    impl ExecutionLimiter {
        pub fn new(n: usize) -> Arc<Self> {
            Arc::new( ExecutionLimiter { count: Mutex::new(0), condvar: Condvar::new(), limit: n} )
        }

        pub fn execute<R>(&self, f: impl Fn() -> R) -> R {
            let try_lock = self.count.lock();
            if try_lock.is_err() { panic!("Error in mutex count!") };
            let mut count = try_lock.unwrap();
            while *count == self.limit {
                println!("Vado a nanna");
                count = self.condvar.wait(count).unwrap();
            }
            (*count) += 1;
            drop(count);
            let _d = Dropper {element: &self };
            self.condvar.notify_all();
            println!("Eseguo");
            f()
        }
    }
}

pub mod ranking_barrier {
    use std::sync::{Arc, Condvar, Mutex};
    use crate::data_structures::ranking_barrier::PhaseEnum::{CLOSE, OPEN};

    #[derive(PartialEq)]
    pub enum PhaseEnum {
        OPEN,
        CLOSE
    }

    pub struct RankingBarrier {
        state: Mutex<(usize, PhaseEnum)>,
        condvar: Condvar,
        limit: usize
    }

    impl RankingBarrier {
        pub fn new(limit: usize) -> Arc<Self> {
            Arc::new( RankingBarrier { state: Mutex::new((0, OPEN)), condvar: Condvar::new(), limit} )
        }

        pub fn wait(&self) -> usize {
            let try_lock = self.state.lock();
            if try_lock.is_err() { panic!("Error in mutex state!") };
            let mut state = try_lock.unwrap();
            loop {
                if (*state).1 == CLOSE && (*state).0 == 0 {
                    println!("Buongiorno! Riapro: {:?}", (*state).0);
                    (*state).1 = OPEN;
                    self.condvar.notify_all();
                    return (*state).0 + 1;
                }
                if (*state).1 == CLOSE && (*state).0 != 0 {
                    println!("Buongiorno!: {:?}", (*state).0);
                    (*state).0 -= 1;
                    self.condvar.notify_all();
                    return (*state).0 + 1;
                }
                if (*state).1 == OPEN && (*state).0 + 1 == self.limit {
                    println!("Tutti svegli!: {:?}", (*state).0);
                    (*state).1 = CLOSE;
                    (*state).0 -= 1;
                    self.condvar.notify_all();
                    return (*state).0 + 1;
                }
                if (*state).1 == OPEN && (*state).0 < self.limit {
                    println!("Vado a nanna: {:?}", (*state).0);
                    (*state).0 += 1;
                    state = self.condvar.wait(state).unwrap();
                }
            }
        }
    }
}

pub mod cache {
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::hash::Hash;
    use std::sync::{Arc, Condvar, Mutex};
    use crate::data_structures::cache::EntryState::{Pending, Ready};

    pub enum EntryState<V> {
        Ready(V),
        Pending
    }

    pub struct Cache<K: Hash + Eq + Clone, V: Debug> {
        map: Mutex<HashMap<K,EntryState<Arc<V>>>>,
        condvar: Condvar
    }

    impl<K: Hash + Eq + Clone, V: Debug> Cache<K,V> {
        pub fn new() -> Arc<Self> {
            Arc::new( Cache { map: Mutex::new(HashMap::new()), condvar: Condvar::new() } )
        }

        pub fn get(&self, k: K, f: impl Fn(K) -> V) -> Arc<V> {
            let try_lock = self.map.lock();
            if try_lock.is_err() { panic!("Errore nel lock di map") };
            let mut map = try_lock.unwrap();
            loop {
                match map.get(&k.clone()) {
                    None => {
                        map.insert(k.clone(), Pending);
                        drop(map);
                        self.condvar.notify_all();
                        let v = Arc::new( f(k.clone()));
                        let mut map2 = self.map.lock().unwrap();
                        map2.insert(k.clone(), Ready(Arc::clone(&v)));
                        drop(map2);
                        self.condvar.notify_all();
                        println!("Nessun valore, trasformo e ritorno: {:?}", *v);
                        return v;
                    }
                    Some(entry) => {
                        match entry {
                            Ready(value) => {
                                println!("Valore trovato!: {:?}", *value);
                                return Arc::clone(value);
                            }
                            Pending => {
                                println!("Valore non trovato, nanna!");
                                map = self.condvar.wait(map).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}

pub mod dispatcher {
    use std::fmt::Debug;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Condvar, Mutex};

    pub struct Subscription<Msg: Send + 'static + Clone + Debug> {
        messages: Receiver<Msg>
    }

    impl<Msg: Send + 'static + Clone + Debug> Subscription<Msg> {
        pub fn new(rx: Receiver<Msg>) -> Self {
            Subscription { messages: rx }
        }

        pub fn read(&self) -> Option<Msg> {
            match self.messages.recv() {
                Ok(msg) => {
                    println!("Leggo: {:?}", msg);
                    Some(msg)
                }
                Err(_) => {
                    println!("Dispathcer distrutto e coda vuota");
                    None
                }
            }
        }
    }

    pub struct Dispatcher<Msg: Send + 'static + Clone + Debug> {
        senders: Mutex<Vec<Sender<Msg>>>
    }

    impl<Msg: Send + 'static + Clone + Debug> Dispatcher<Msg> {
        pub fn new() -> Arc<Self> {
            Arc::new( Dispatcher { senders: Mutex::new(Vec::new()) } )
        }

        pub fn dispatch(&self, msg: Msg) {
            let try_lock = self.senders.lock();
            if try_lock.is_err() { panic!("Errore nel lock di senders") };
            let mut senders = try_lock.unwrap();
            for i in 0..senders.len() {
                println!("Invio: {:?}", msg);
                match senders.get(i).unwrap().send(msg.clone()) {
                    Ok(msg) => {
                    }
                    Err(_) => {
                        println!("Rimuovo il sender: {:?}", i);
                        senders.remove(i);
                    }
                }
            }
            drop(senders);
        }

        pub fn subscribe(&self) -> Subscription<Msg> {
            let try_lock = self.senders.lock();
            if try_lock.is_err() { panic!("Errore nel lock di senders") };
            let mut senders = try_lock.unwrap();
            let (tx, rx) = channel();
            senders.push(tx);
            drop(senders);
            println!("Subscription tornata!");
            Subscription::new(rx)
        }
    }

    impl<Msg: Send + 'static + Clone + Debug> Drop for Dispatcher<Msg> {
        fn drop(&mut self) {
            println!("Dispatcher distrutto")
        }
    }
}

pub mod delayed_queue {
    use std::collections::VecDeque;
    use std::fmt::Debug;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Instant;

    pub struct Message<T: Send + 'static + Clone + Debug> {
        element: T,
        instant: Instant
    }

    impl<T: Send + 'static + Clone + Debug> Message<T> {
        pub fn new(element: T, instant: Instant) -> Self {
            Message {element, instant}
        }
    }

    pub struct DelayedQueue<T: Send + 'static + Clone + Debug> {
        queue: Mutex<VecDeque<Message<T>>>,
        condvar: Condvar
    }

    impl<T: Send + 'static + Clone + Debug> DelayedQueue<T> {
        pub fn new() -> Arc<Self> {
            Arc::new( DelayedQueue { queue: Mutex::new(VecDeque::new()), condvar: Condvar::new() } )
        }

        pub fn offer(&self, t: T, i: Instant) {
            let try_lock = self.queue.lock();
            if try_lock.is_err() { panic!("Errore nel lock di queue") };
            let mut queue = try_lock.unwrap();
            for j in 0..queue.len() {
                if i < queue.get(j).unwrap().instant {
                    queue.insert(j , Message::new(t.clone(),i));
                    break;
                } else if j == queue.len() - 1 {
                    queue.push_back(Message::new(t.clone(),i));
                    break;
                }
            }
            if queue.len() == 0 {
                queue.push_front(Message::new(t.clone(),i));
            }
            println!("L'array ora è:");
            for k in 0..queue.len() {
                println!("Elemento {}: {:?}", k, queue.get(k).unwrap().element );
            }
            drop(queue);
            self.condvar.notify_all();
        }

        pub fn take(&self) -> Option<T> {
            let try_lock = self.queue.lock();
            if try_lock.is_err() { panic!("Errore nel lock di queue") };
            let mut queue = try_lock.unwrap();
            loop {
                if queue.len() == 0 {
                    drop(queue);
                    println!("Ho trovato ciole :(");
                    return None;
                }
                if queue.front().unwrap().instant < Instant::now() {
                    let to_ret = queue.pop_front().unwrap().element;
                    drop(queue);
                    println!("Estraggo elemento: {:?}", to_ret);
                    return Some(to_ret);
                }
                let timeout = queue.front().unwrap().instant - Instant::now();
                println!("Mancano ancora {:?}... nanna", timeout.as_secs());
                queue = self.condvar.wait_timeout(queue, timeout).unwrap().0;
                println!("Buongiorno!!!")
            }
        }

        pub fn size(&self) -> usize {
            let try_lock = self.queue.lock();
            if try_lock.is_err() { panic!("Errore nel lock di queue") };
            let mut queue = try_lock.unwrap();
            return queue.len();
        }
    }
}

pub mod mpmc_channel {
    use std::collections::VecDeque;
    use std::fmt::Debug;
    use std::sync::{Arc, Condvar, Mutex};
    use crate::data_structures::mpmc_channel::BufferStatus::{CLOSE, OPEN};

    #[derive(PartialEq)]
    pub enum BufferStatus {
        OPEN,
        CLOSE
    }

    pub struct MpMcChannel<E:Send + 'static + Clone + Debug> {
        messages: Mutex<(VecDeque<E>, BufferStatus)>,
        condvar: Condvar,
        size: usize
    }

    impl<E:Send + 'static + Clone + Debug> MpMcChannel<E> {
        pub fn new(size: usize) -> Arc<Self> {
            Arc::new( MpMcChannel { messages: Mutex::new((VecDeque::with_capacity(size), OPEN)), condvar: Condvar::new(), size} )
        }

        pub fn send(&self, e: E) -> Option<()> {
            let try_lock = self.messages.lock();
            if try_lock.is_err() { panic!("Error locking messages") };
            let mut messages = try_lock.unwrap();
            loop {
                if messages.1 == CLOSE {
                    println!("Volevo inviare ma ciole :(");
                    drop(messages);
                    return None;
                }
                if messages.0.len() < self.size {
                    println!("Invio: {:?}", e);
                    messages.0.push_back(e.clone());
                    drop(messages);
                    self.condvar.notify_all();
                    return Some(());
                }
                println!("Canale pieno attendo...");
                messages = self.condvar.wait(messages).unwrap();
            }
        }

        pub fn recv(&self) -> Option<E> {
            let try_lock = self.messages.lock();
            if try_lock.is_err() { panic!("Error locking messages") };
            let mut messages = try_lock.unwrap();
            loop {
                if messages.1 == CLOSE && messages.0.len() == 0{
                    println!("Volevo ricevere ma ciole :(");
                    drop(messages);
                    return None;
                }
                if messages.0.len() > 0 {
                    let e = messages.0.pop_front().unwrap();
                    println!("Ricevo: {:?}", e);
                    drop(messages);
                    self.condvar.notify_all();
                    return Some(e);
                }
                println!("Canale vuoto attendo...");
                messages = self.condvar.wait(messages).unwrap();
            }
        }

        pub fn shutdown(&self) -> Option<()> {
            let try_lock = self.messages.lock();
            if try_lock.is_err() { panic!("Error locking messages") };
            let mut messages = try_lock.unwrap();
            println!("Chiudo il canale");
            messages.1 = CLOSE;
            return Some(());
        }
    }
}