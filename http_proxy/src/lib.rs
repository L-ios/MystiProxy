mod gateway;
mod mocker;

#[cfg(feature = "local-management")]
pub mod management;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(4, 4);
    }
}
