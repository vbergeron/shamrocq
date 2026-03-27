(define nat_ord (lambdas (x y)
  (match x
    ((O) (match y
      ((O) `(Left))
      ((S _) `(Left))))
    ((S x~) (match y
      ((O) `(Right))
      ((S y~) (@ nat_ord x~ y~)))))))
