(define append (lambdas (l1 l2)
  (match l1
    ((Nil) l2)
    ((Cons x xs) `(Cons ,x ,(@ append xs l2))))))

(define rev_aux (lambdas (acc l)
  (match l
    ((Nil) acc)
    ((Cons x xs) (@ rev_aux `(Cons ,x ,acc) xs)))))

(define reverse (lambda (l) (@ rev_aux `(Nil) l)))

(define nth (lambdas (n l)
  (match l
    ((Nil) `(None_))
    ((Cons x xs) (match n
      ((O) `(Some ,x))
      ((S n~) (@ nth n~ xs)))))))

(define zip (lambdas (l1 l2)
  (match l1
    ((Nil) `(Nil))
    ((Cons x xs) (match l2
      ((Nil) `(Nil))
      ((Cons y ys) `(Cons ,`(Pair ,x ,y) ,(@ zip xs ys))))))))
