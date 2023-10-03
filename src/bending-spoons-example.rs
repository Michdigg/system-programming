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
