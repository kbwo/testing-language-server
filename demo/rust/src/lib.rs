fn hello() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    fn not_test() {}

    #[test]
    fn success() {
        assert!(true);
    }

    #[test]
    fn fail() {
        assert!(false);
    }

    #[tokio::test]
    async fn tokio_test_success() {
        assert!(true);
    }

    #[tokio::test]
    async fn tokio_test_fail() {
        assert!(false);
    }

    mod nested_namespace {
        fn not_test() {}

        #[test]
        fn success() {
            assert!(true);
        }

        #[test]
        fn fail() {
            assert!(false);
        }

        mod nested_nested_namespace {
            fn not_test() {}

            #[test]
            fn success() {
                assert!(true);
            }

            #[test]
            fn fail() {
                assert!(false);
            }
        }
    }
}
