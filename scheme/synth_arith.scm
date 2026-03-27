(define add (lambdas (n m)
  (match n
    ((O) m)
    ((S n~) `(S ,(@ add n~ m))))))

(define mul (lambdas (n m)
  (match n
    ((O) `(O))
    ((S n~) (@ add m (@ mul n~ m))))))

(define sub (lambdas (n m)
  (match m
    ((O) n)
    ((S m~) (match n
      ((O) `(O))
      ((S n~) (@ sub n~ m~)))))))

(define min_nat (lambdas (n m)
  (match n
    ((O) `(O))
    ((S n~) (match m
      ((O) `(O))
      ((S m~) `(S ,(@ min_nat n~ m~))))))))

(define max_nat (lambdas (n m)
  (match n
    ((O) m)
    ((S n~) (match m
      ((O) n)
      ((S m~) `(S ,(@ max_nat n~ m~))))))))
